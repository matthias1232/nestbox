package main

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"math/big"
	"testing"
	"time"
)

// testCA builds an in-memory CA shaped like the one make-egress-ca.sh writes:
// EC P-256, basic constraints CA, path length zero.
func testCA(t *testing.T) (*authority, *x509.Certificate) {
	t.Helper()

	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatalf("generate CA key: %v", err)
	}
	template := &x509.Certificate{
		SerialNumber:          big.NewInt(1),
		Subject:               pkix.Name{CommonName: "nestbox-egress-test"},
		NotBefore:             time.Now().Add(-time.Hour),
		NotAfter:              time.Now().Add(time.Hour),
		KeyUsage:              x509.KeyUsageCertSign,
		BasicConstraintsValid: true,
		IsCA:                  true,
		MaxPathLenZero:        true,
	}
	der, err := x509.CreateCertificate(rand.Reader, template, template, key.Public(), key)
	if err != nil {
		t.Fatalf("sign CA: %v", err)
	}
	caCert, err := x509.ParseCertificate(der)
	if err != nil {
		t.Fatalf("parse CA: %v", err)
	}

	leafKey, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatalf("generate leaf key: %v", err)
	}
	return &authority{caCert: caCert, caKey: key, leafKey: leafKey, cache: map[string]*tls.Certificate{}}, caCert
}

func TestMintSignsForTheRequestedHost(t *testing.T) {
	a, caCert := testCA(t)

	leaf, err := a.mint("mcp.example.com")
	if err != nil {
		t.Fatalf("mint: %v", err)
	}
	parsed, err := x509.ParseCertificate(leaf.Certificate[0])
	if err != nil {
		t.Fatalf("parse leaf: %v", err)
	}

	if parsed.IsCA {
		t.Fatal("leaf is a CA certificate")
	}
	if len(parsed.DNSNames) != 1 || parsed.DNSNames[0] != "mcp.example.com" {
		t.Fatalf("DNS names = %v, want [mcp.example.com]", parsed.DNSNames)
	}
	if err := parsed.CheckSignatureFrom(caCert); err != nil {
		t.Fatalf("leaf is not signed by the CA: %v", err)
	}
	if time.Until(parsed.NotAfter) <= 0 {
		t.Fatal("leaf is already expired")
	}
	// The client verifies the host name against the certificate, so this is
	// the check that actually decides whether a sandbox accepts the tunnel.
	if err := parsed.VerifyHostname("mcp.example.com"); err != nil {
		t.Fatalf("VerifyHostname: %v", err)
	}
}

func TestMintUsesAnIPSANForAddresses(t *testing.T) {
	a, _ := testCA(t)

	leaf, err := a.mint("192.0.2.10")
	if err != nil {
		t.Fatalf("mint: %v", err)
	}
	parsed, err := x509.ParseCertificate(leaf.Certificate[0])
	if err != nil {
		t.Fatalf("parse leaf: %v", err)
	}
	if len(parsed.IPAddresses) != 1 || parsed.IPAddresses[0].String() != "192.0.2.10" {
		t.Fatalf("IP SANs = %v, want [192.0.2.10]", parsed.IPAddresses)
	}
	if len(parsed.DNSNames) != 0 {
		t.Fatalf("DNS names = %v, want none", parsed.DNSNames)
	}
}

func TestCertificateForCachesPerHost(t *testing.T) {
	a, _ := testCA(t)

	first, err := a.certificateFor("example.com")
	if err != nil {
		t.Fatalf("first mint: %v", err)
	}
	second, err := a.certificateFor("example.com")
	if err != nil {
		t.Fatalf("second mint: %v", err)
	}
	if first != second {
		t.Fatal("the cache returned a different certificate for the same host")
	}

	other, err := a.certificateFor("example.org")
	if err != nil {
		t.Fatalf("mint for a different host: %v", err)
	}
	if other == first {
		t.Fatal("two hosts share one certificate")
	}
}
