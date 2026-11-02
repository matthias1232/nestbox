package main

import "net/http"

// tapLive watches a streamed answer and reports it live.
//
// Stub: G-3 implements it. The hook exists so the response path has exactly one
// place where a live report is produced, and so the work package that builds it
// does not have to touch the proxy itself.
func tapLive(resp *http.Response) { _ = resp }

// liveInfoKey carries the sandbox and the request id to the response side.
type liveInfoKey struct{}

// liveInfo is what a live report needs to know about the request.
type liveInfo struct {
	SandboxID  string
	RequestRef string
}
