package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"log"
	"net/http"
	"os"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// The content check on the decrypted hop.
//
// The proxy decides *whether* a request may leave; this decides *what* may
// leave. It sits here and not in Envoy's Lua filter because rewriting a body is
// a first-class operation on an `io.Reader` and a guess in a filter without a
// JSON library: a filter that edits a payload it only pattern-matched would be a
// filter that corrupts requests and cannot say why.
//
// The rule is the control API's, not this file's: this hop sends the body and the
// address of the request and does what the answer says — refuse, forward the
// rewritten text, or forward unchanged. `observe` is the one mode that may carry
// on when the control API cannot answer; everywhere else an unanswered check is
// a refusal, because a filter that fails open is a filter an outage turns off.
//
// Only requests are inspected. An answer that streams back token by token
// cannot be rewritten without breaking the stream, so the outbound direction is
// recorded by the accounting proxy when the agent routes through it, and
// otherwise not at all. The console says that plainly instead of implying
// otherwise.

// maxInspectedBody is the same 4 MiB Envoy buffers on the terminated listener.
// A larger body is forwarded uninspected rather than truncated: half a JSON
// document is not a smaller document.
const maxInspectedBody = 4 << 20

// dlpTimeout bounds the check. The control API answers from one row and a
// regex set; a slow answer must not become a slow request.
const dlpTimeout = 1500 * time.Millisecond

// dlpClient asks the control API about one body.
type dlpClient struct {
	url    string
	nodeID string
	token  string
	client *http.Client
}

// newDlpClientFromEnv builds the client from the proxy's own identity.
//
// The same node id and token the Lua filter uses for its tunnel check: the
// control API already trusts them, and a second credential would be a second
// thing to rotate.
func newDlpClientFromEnv() *dlpClient {
	nodeID := strings.TrimSpace(os.Getenv("NESTBOX_NODE_ID"))
	token := strings.TrimSpace(os.Getenv("NESTBOX_NODE_TOKEN"))
	if token == "" {
		if path := strings.TrimSpace(os.Getenv("NESTBOX_NODE_TOKEN_FILE")); path != "" {
			if raw, err := os.ReadFile(path); err == nil {
				token = strings.TrimSpace(string(raw))
			}
		}
	}
	url := strings.TrimSpace(os.Getenv("NESTBOX_CONTROL_URL"))
	if url == "" {
		url = "http://control-api:8000"
	}
	if nodeID == "" || token == "" {
		log.Printf("egress-dlp: no node identity; bodies are forwarded without a content check")
		return nil
	}
	return &dlpClient{
		url:    strings.TrimRight(url, "/"),
		nodeID: nodeID,
		token:  token,
		client: &http.Client{Timeout: dlpTimeout},
	}
}

// dlpRequest is one body and the request it belongs to.
type dlpRequest struct {
	SandboxID      string `json:"sandbox_id"`
	Content        string `json:"content"`
	Destination    string `json:"destination,omitempty"`
	Source         string `json:"source"`
	Method         string `json:"method,omitempty"`
	Path           string `json:"path,omitempty"`
	Model          string `json:"model,omitempty"`
	McpTool        string `json:"mcp_tool,omitempty"`
	ConversationID string `json:"conversation_id,omitempty"`
}

// dlpFinding is what the console lists: kind, masked excerpt, what happened.
type dlpFinding struct {
	Detector    string `json:"detector"`
	MaskedValue string `json:"masked_value"`
	Action      string `json:"action"`
	Requested   string `json:"requested"`
}

// dlpVerdict is the control API's answer.
type dlpVerdict struct {
	Permitted          bool         `json:"permitted"`
	Action             string       `json:"action"`
	TransformedContent string       `json:"transformed_content"`
	Findings           []dlpFinding `json:"findings"`
}

// inspect sends one body and returns the verdict.
func (c *dlpClient) inspect(ctx context.Context, body dlpRequest) (dlpVerdict, error) {
	payload, err := json.Marshal(body)
	if err != nil {
		return dlpVerdict{}, err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.url+"/api/v1/internal/privacy/inspect", bytes.NewReader(payload))
	if err != nil {
		return dlpVerdict{}, err
	}
	req.Header.Set("content-type", "application/json")
	req.Header.Set("x-nestbox-node", c.nodeID)
	req.Header.Set("authorization", "Bearer "+c.token)
	resp, err := c.client.Do(req)
	if err != nil {
		return dlpVerdict{}, err
	}
	defer func() { _ = resp.Body.Close() }()
	raw, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return dlpVerdict{}, err
	}
	if resp.StatusCode != http.StatusOK {
		return dlpVerdict{}, errors.New("inspect answered " + resp.Status)
	}
	var verdict dlpVerdict
	if err := json.Unmarshal(raw, &verdict); err != nil {
		return dlpVerdict{}, err
	}
	return verdict, nil
}

// apply checks one request and either lets it through (possibly rewritten) or
// writes the refusal itself and reports handled.
//
// A request this hop cannot check — no sandbox, a body it must not read, a
// non-JSON content type — is forwarded, because the policy decision that let it
// through has already been made by Envoy. What it must never do is forward a
// body it read and then failed to check while the mode forbids that; that case
// is the 503 below.
func (c *dlpClient) apply(w http.ResponseWriter, r *http.Request, mode string) (handled bool) {
	if r.Body == nil {
		return false
	}
	log.Printf("egress-dlp: entered %s %s ct=%q len=%d sandbox=%q", r.Method, r.Host, r.Header.Get("content-type"), r.ContentLength, r.Header.Get("x-nestbox-sandbox"))
	switch r.Method {
	case http.MethodPost, http.MethodPut, http.MethodPatch:
	default:
		return false
	}
	if !strings.Contains(strings.ToLower(r.Header.Get("content-type")), "json") {
		return false
	}
	sandboxID := strings.TrimSpace(r.Header.Get("x-nestbox-sandbox"))
	if sandboxID == "" {
		// Envoy copies the id out of the policy answer. An empty one means the
		// request was not identified, and a body nobody can be blamed for is not
		// a body this hop may refuse. Say so once per request: silence here was
		// how "the check ran and found nothing" looked exactly like "the check
		// never ran".
		log.Printf("egress-dlp: %s %s: no sandbox in the request; body not checked", r.Method, r.Host)
		return false
	}
	if r.ContentLength > maxInspectedBody {
		return false
	}
	original, err := io.ReadAll(io.LimitReader(r.Body, maxInspectedBody+1))
	if err != nil {
		return false
	}
	_ = r.Body.Close()
	if len(original) == 0 || len(original) > maxInspectedBody {
		r.Body = io.NopCloser(bytes.NewReader(original))
		return false
	}
	r.Body = io.NopCloser(bytes.NewReader(original))

	body := dlpRequest{
		SandboxID:   sandboxID,
		Content:     string(original),
		Destination: r.Host,
		Source:      "egress",
		Method:      r.Method,
		Path:        r.URL.Path,
	}
	text := string(original)
	body.Model = jsonField(text, `"model"\s*:\s*"([^"]+)"`)
	body.ConversationID = jsonField(text, `session_id\\?"\s*:\s*\\?"([0-9a-fA-F-]{8,})`)
	if strings.Contains(text, `"jsonrpc"`) {
		body.Source = "mcp"
		body.McpTool = jsonField(text, `"name"\s*:\s*"([^"]+)"`)
	}

	ctx, cancel := context.WithTimeout(r.Context(), dlpTimeout)
	defer cancel()
	verdict, err := c.inspect(ctx, body)
	if err == nil {
		// One line per request that found something. A clean request stays quiet,
		// because a log nobody can read is a log nobody reads.
		if len(verdict.Findings) > 0 || !verdict.Permitted {
			log.Printf("egress-dlp: %s %s: %s (%d findings)", r.Method, r.Host, verdict.Action, len(verdict.Findings))
		}
	}
	if err != nil {
		if mode == "observe" {
			log.Printf("egress-dlp: %s: %v; observe mode forwards", r.Host, err)
			return false
		}
		log.Printf("egress-dlp: %s: %v; refusing", r.Host, err)
		writeDlpRefusal(w, "privacy.unavailable", "The content check could not be answered, so the request was refused.")
		return true
	}
	if !verdict.Permitted {
		writeDlpRefusal(w, "privacy.blocked", "The content policy refused this request.")
		return true
	}
	if verdict.Action == "replaced" && verdict.TransformedContent != "" && verdict.TransformedContent != string(original) {
		replaced := []byte(verdict.TransformedContent)
		r.Body = io.NopCloser(bytes.NewReader(replaced))
		r.ContentLength = int64(len(replaced))
		r.Header.Set("content-length", strconv.Itoa(len(replaced)))
	}
	return false
}

// writeDlpRefusal answers the sandbox with a reason it can act on.
//
// No matched value is in the body; a detector name and a count are enough to
// say why, and enough is all that may be said.
func writeDlpRefusal(w http.ResponseWriter, code, message string) {
	w.Header().Set("content-type", "application/json")
	w.WriteHeader(http.StatusForbidden)
	_ = json.NewEncoder(w).Encode(map[string]any{
		"error": map[string]any{"code": code, "message": message},
	})
}

// jsonField returns the first match of a pattern, or "".
func jsonField(text, pattern string) string {
	match := regexp.MustCompile(pattern).FindStringSubmatch(text)
	if len(match) < 2 {
		return ""
	}
	return match[1]
}
