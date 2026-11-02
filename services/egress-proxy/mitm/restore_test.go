package main

import (
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"testing"
)

// The answer goes back to an agent that has to keep working, so a placeholder in
// it must become the value again — in a whole JSON answer, in a streamed text
// delta, and in a streamed tool input that is itself JSON.

func response(contentType, body string) *http.Response {
	return &http.Response{
		StatusCode:    200,
		Header:        http.Header{"Content-Type": []string{contentType}},
		Body:          io.NopCloser(strings.NewReader(body)),
		ContentLength: int64(len(body)),
		Request:       &http.Request{},
	}
}

func TestRestoreInAWholeJsonAnswer(t *testing.T) {
	pairs := []restorePair{{Placeholder: "<PERSON_NAMES_1a2b3c4d>", Value: `Max "M" Mustermann`}}
	resp := response("application/json", `{"content":[{"type":"text","text":"Hallo <PERSON_NAMES_1a2b3c4d>!"}]}`)
	if err := restoreResponse(resp, pairs); err != nil {
		t.Fatal(err)
	}
	got, _ := io.ReadAll(resp.Body)
	want := `{"content":[{"type":"text","text":"Hallo Max \"M\" Mustermann!"}]}`
	if string(got) != want {
		t.Fatalf("body %q, want %q", got, want)
	}
	if resp.ContentLength != int64(len(want)) {
		t.Fatalf("content length %d does not match %d bytes", resp.ContentLength, len(want))
	}
}

func TestRestoreInAStreamedTextDelta(t *testing.T) {
	pairs := []restorePair{{Placeholder: "<IBAN_faf7e1c0>", Value: "DE89370400440532013000"}}
	stream := strings.Join([]string{
		`data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Die IBAN ist <IBAN_"}}`,
		`data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"faf7e1c0>."}}`,
		`data: {"type":"content_block_stop","index":0}`,
		"",
	}, "\n")
	resp := response("text/event-stream", stream)
	if err := restoreResponse(resp, pairs); err != nil {
		t.Fatal(err)
	}
	got, _ := io.ReadAll(resp.Body)
	// The placeholder was split over two deltas; the client concatenates the
	// text, so the assertion does too.
	var text strings.Builder
	for _, line := range strings.Split(string(got), "\n") {
		payload := strings.TrimPrefix(line, "data: ")
		if payload == "" || payload == line {
			continue
		}
		var event map[string]any
		if err := json.Unmarshal([]byte(payload), &event); err != nil {
			continue
		}
		if delta, ok := event["delta"].(map[string]any); ok {
			if value, ok := delta["text"].(string); ok {
				text.WriteString(value)
			}
		}
	}
	if text.String() != "Die IBAN ist DE89370400440532013000." {
		t.Fatalf("reassembled text %q", text.String())
	}
}

func TestRestoreInAStreamedToolInput(t *testing.T) {
	pairs := []restorePair{{Placeholder: "<HOSTNAMES_1234abcd>", Value: `intranet.firma.local`}}
	stream := `data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"url\":\"<HOSTNAMES_1234abcd>\"}"}}` + "\n"
	resp := response("text/event-stream", stream)
	if err := restoreResponse(resp, pairs); err != nil {
		t.Fatal(err)
	}
	got, _ := io.ReadAll(resp.Body)
	if !strings.Contains(string(got), "intranet.firma.local") {
		t.Fatalf("tool input was not restored: %q", got)
	}
}

func TestRestoreLeavesUnknownPlaceholders(t *testing.T) {
	resp := response("application/json", `{"text":"<OTHER_ffffffff>"}`)
	if err := restoreResponse(resp, []restorePair{{Placeholder: "<IBAN_1>", Value: "x"}}); err != nil {
		t.Fatal(err)
	}
	got, _ := io.ReadAll(resp.Body)
	if string(got) != `{"text":"<OTHER_ffffffff>"}` {
		t.Fatalf("body changed to %q", got)
	}
}

func TestRestoreLeavesOtherContentTypesAlone(t *testing.T) {
	original := `{"text":"<IBAN_1>"}`
	resp := response("text/plain", original)
	if err := restoreResponse(resp, []restorePair{{Placeholder: "<IBAN_1>", Value: "x"}}); err != nil {
		t.Fatal(err)
	}
	got, _ := io.ReadAll(resp.Body)
	if string(got) != original {
		t.Fatalf("body changed to %q", got)
	}
}
