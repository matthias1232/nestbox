package main

import "net/http"

// traceUpstream records which address the request reached.
//
// Stub: G-2 implements it. The console shows the address a request actually went
// to, and, for a refused request, the one it would have reached.
func traceUpstream(r *http.Request) *http.Request { return r }

// annotateResponse adds the upstream address and the router's own headers.
//
// Stub: G-2 implements it.
func annotateResponse(resp *http.Response) { _ = resp }
