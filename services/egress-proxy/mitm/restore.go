package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"sort"
	"strconv"
	"strings"
)

// Turn placeholders back into the values they stood for.
//
// With `pseudonymize` the model never sees a real value: it sees
// `<PERSON_NAMES_1a2b3c4d>` and answers with it. The agent must still be able to
// work, so the answer is restored on the way back — here, in the hop that holds
// the plaintext. The list of pairs lives only in the memory of this request and
// is never written anywhere (privacy module, section 6).
//
// Two shapes have to be handled, and both split the placeholder across chunks:
//
//   - a whole JSON answer, where the placeholder sits inside a string literal, so
//     the value has to be escaped as JSON string content;
//   - server-sent events, where `delta.text` is plain and `delta.partial_json`
//     carries a JSON fragment that is itself escaped, so the value needs one more
//     level of escaping.
//
// A delta can end in the middle of a placeholder, because the model emits one
// token at a time. The tail that could still become a placeholder is held back
// and prefixed to the next delta of the same block; a block end flushes it.

// maxRestoredAnswer is the same ceiling the request side uses: a bigger answer is
// not buffered, it is streamed through unchanged.
const maxRestoredAnswer = 8 << 20

// placeholderTail reports whether `value` can still turn into a placeholder:
// a `<` followed only by what a placeholder is made of.
func placeholderTail(value string) bool {
	if !strings.HasPrefix(value, "<") || len(value) > 40 {
		return false
	}
	for _, character := range value[1:] {
		if (character < 'A' || character > 'Z') && character != '_' && (character < '0' || character > '9') {
			return false
		}
	}
	return true
}

// restorer replaces placeholders in one answer.
type restorer struct {
	pairs []restorePair
	// carry holds the tail of a streamed value per block and kind, keyed
	// "<index>:<text|json>".
	carry map[string]string
}

func newRestorer(pairs []restorePair) *restorer {
	ordered := make([]restorePair, len(pairs))
	copy(ordered, pairs)
	// Longest first: a placeholder is a prefix of no other, but a value could
	// contain one by accident.
	sort.Slice(ordered, func(left, right int) bool {
		return len(ordered[left].Placeholder) > len(ordered[right].Placeholder)
	})
	return &restorer{pairs: ordered, carry: map[string]string{}}
}

// value returns the replacement for one chunk, or "" and false.
//
// `fragment` says the text is JSON inside JSON, which needs one more escape.
func (r *restorer) value(chunk string, fragment bool) (string, bool) {
	replaced := chunk
	found := false
	for _, pair := range r.pairs {
		if !strings.Contains(replaced, pair.Placeholder) {
			continue
		}
		replacement := pair.Value
		if fragment {
			replacement = escapeInner(replacement)
		}
		replaced = strings.ReplaceAll(replaced, pair.Placeholder, replacement)
		found = true
	}
	return replaced, found
}

// restoreBody rewrites one whole JSON answer in place.
func (r *restorer) restoreBody(raw string) string {
	out, _ := r.value(raw, true)
	return out
}

// restoreChunk handles one streamed piece, holding a tail that could still
// become a placeholder.
func (r *restorer) restoreChunk(key, chunk string, fragment bool) (string, bool) {
	combined := r.carry[key] + chunk
	delete(r.carry, key)
	cut := strings.LastIndex(combined, "<")
	if cut >= 0 && placeholderTail(combined[cut:]) {
		r.carry[key] = combined[cut:]
		combined = combined[:cut]
	}
	out, found := r.value(combined, fragment)
	if out == "" && !found {
		return "", false
	}
	return out, true
}

// flush returns a held tail when a block ends.
func (r *restorer) flush(key string) (string, bool) {
	tail, held := r.carry[key]
	delete(r.carry, key)
	return tail, held
}

// escapeInner escapes a value for use inside a JSON string literal.
func escapeInner(value string) string {
	quoted := strconv.Quote(value)
	return quoted[1 : len(quoted)-1]
}

// restoreResponse rewrites a response before it reaches the sandbox.
func restoreResponse(resp *http.Response, pairs []restorePair) error {
	if len(pairs) == 0 {
		return nil
	}
	restorer := newRestorer(pairs)
	contentType := strings.ToLower(resp.Header.Get("content-type"))

	if strings.Contains(contentType, "text/event-stream") {
		reader, writer := io.Pipe()
		original := resp.Body
		resp.Body = reader
		resp.ContentLength = -1
		resp.Header.Del("content-length")
		go func() {
			defer func() { _ = writer.Close() }()
			defer func() { _ = original.Close() }()
			_ = restorer.copyEvents(original, writer)
		}()
		return nil
	}

	if !strings.Contains(contentType, "json") {
		return nil
	}
	if resp.ContentLength > maxRestoredAnswer {
		return nil
	}
	body, err := io.ReadAll(io.LimitReader(resp.Body, maxRestoredAnswer+1))
	if err != nil {
		return err
	}
	_ = resp.Body.Close()
	if len(body) > maxRestoredAnswer {
		resp.Body = io.NopCloser(bytes.NewReader(body))
		return nil
	}
	restored := restorer.restoreBody(string(body))
	resp.Body = io.NopCloser(strings.NewReader(restored))
	resp.ContentLength = int64(len(restored))
	resp.Header.Set("content-length", strconv.Itoa(len(restored)))
	return nil
}

// copyEvents reads server-sent events one line at a time and writes them on
// immediately, so the first token still arrives first.
func (r *restorer) copyEvents(source io.Reader, sink io.Writer) error {
	scanner := bufio.NewScanner(source)
	scanner.Buffer(make([]byte, 0, 64*1024), 4<<20)
	for scanner.Scan() {
		line := scanner.Text()
		out := r.restoreLine(line)
		if _, err := io.WriteString(sink, out+"\n"); err != nil {
			return err
		}
	}
	return scanner.Err()
}

// restoreLine rewrites one SSE line, leaving everything unrecognised untouched.
func (r *restorer) restoreLine(line string) string {
	if !strings.HasPrefix(line, "data:") {
		return line
	}
	payload := strings.TrimSpace(strings.TrimPrefix(line, "data:"))
	if payload == "" || payload == "[DONE]" {
		return line
	}
	var event map[string]any
	if err := json.Unmarshal([]byte(payload), &event); err != nil {
		return line
	}
	changed := false

	index := 0
	if raw, ok := event["index"].(float64); ok {
		index = int(raw)
	}

	if delta, ok := event["delta"].(map[string]any); ok {
		// `text` is the answer as a person reads it: a placeholder can be split
		// between deltas, so the tail is held and prefixed to the next one.
		if text, ok := delta["text"].(string); ok {
			if out, touched := r.restoreChunk(streamKey(index, "text"), text, false); touched {
				delta["text"] = out
				changed = true
			}
		}
		// `partial_json` is a JSON fragment that the client assembles; carrying a
		// tail over would hand it half a document, so a split placeholder stays
		// as it is rather than corrupting a tool call.
		if fragment, ok := delta["partial_json"].(string); ok {
			if out, touched := r.value(fragment, true); touched {
				delta["partial_json"] = out
				changed = true
			}
		}
	}

	// A block that ends while a tail was held ends the placeholder too: it was
	// never completed, so it is the text as it was.
	if kind, ok := event["type"].(string); ok && kind == "content_block_stop" {
		if tail, held := r.flush(streamKey(index, "text")); held {
			event["delta"] = map[string]any{"type": "text_delta", "text": tail}
			changed = true
		}
	}

	if !changed {
		return line
	}
	encoded, err := json.Marshal(event)
	if err != nil {
		return line
	}
	return "data: " + string(encoded)
}

func streamKey(index int, kind string) string {
	return strconv.Itoa(index) + ":" + kind
}
