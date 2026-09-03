package oresapidocs

import (
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

type fixtureDocument struct {
	SchemaVersion int    `json:"schemaVersion"`
	Profile       string `json:"profile"`
	Valid         []struct {
		Name         string `json:"name"`
		Kind         string `json:"kind"`
		Encoded      string `json:"encoded"`
		TCPPrefixHex string `json:"tcp_prefix_hex"`
	} `json:"valid"`
	Invalid []struct {
		Name    string `json:"name"`
		Kind    string `json:"kind"`
		Encoded string `json:"encoded"`
	} `json:"invalid"`
}

func TestSharedFixturesRoundTrip(t *testing.T) {
	data, err := os.ReadFile(filepath.FromSlash("../../examples/rpc-v1/conformance.json"))
	if err != nil {
		t.Fatal(err)
	}
	var document fixtureDocument
	if err := json.Unmarshal(data, &document); err != nil {
		t.Fatal(err)
	}
	if document.SchemaVersion != 1 || document.Profile != "ores-rpc-v1-call-receipt" {
		t.Fatal("fixture profile drift")
	}
	for _, fixture := range document.Valid {
		t.Run(fixture.Name, func(t *testing.T) {
			var encoded []byte
			if fixture.Kind == "call" {
				value, err := DecodeCall([]byte(fixture.Encoded))
				if err != nil {
					t.Fatal(err)
				}
				encoded, err = value.Encode()
				if err != nil {
					t.Fatal(err)
				}
			} else {
				value, err := DecodeReceipt([]byte(fixture.Encoded))
				if err != nil {
					t.Fatal(err)
				}
				encoded, err = value.Encode()
				if err != nil {
					t.Fatal(err)
				}
			}
			if string(encoded) != fixture.Encoded {
				t.Fatalf("\nwant %s\n got %s", fixture.Encoded, encoded)
			}
			framed := make([]byte, 4+len(encoded))
			copy(framed[4:], encoded)
			framed[0] = byte(len(encoded) >> 24)
			framed[1] = byte(len(encoded) >> 16)
			framed[2] = byte(len(encoded) >> 8)
			framed[3] = byte(len(encoded))
			if hex.EncodeToString(framed[:4]) != fixture.TCPPrefixHex {
				t.Fatal("prefix drift")
			}
		})
	}
	for _, fixture := range document.Invalid {
		t.Run(fixture.Name, func(t *testing.T) {
			var err error
			if fixture.Kind == "call" {
				_, err = DecodeCall([]byte(fixture.Encoded))
			} else {
				_, err = DecodeReceipt([]byte(fixture.Encoded))
			}
			if err == nil {
				t.Fatalf("accepted invalid fixture %s", fixture.Encoded)
			}
		})
	}
}

func TestReceiptStateMachineAndCorrelation(t *testing.T) {
	invalid := [][]byte{
		[]byte(`{"v":1,"op":"receipt","id":"c","key":"get_item","ok":false}`),
		[]byte(`{"v":1,"op":"receipt","id":"c","key":"get_item","ok":true,"error":{"code":"bad"}}`),
		[]byte(`{"v":1,"op":"receipt","id":"c","key":"get_item","ok":false,"status":200,"error":{"code":"bad"}}`),
		[]byte(`{"v":1,"op":"receipt","id":"c","key":"get_item","ok":false,"status":500,"body":null,"error":{"code":"bad"}}`),
	}
	for _, payload := range invalid {
		if _, err := DecodeReceipt(payload); err == nil {
			t.Fatalf("accepted %s", payload)
		}
	}
	call := NewCall("c1", "get_item")
	result := SuccessReceipt("c2", "get_item", 200, OptionalJSON{})
	if err := AssertReceiptForCall(call, result); err == nil || !strings.Contains(err.Error(), "id") {
		t.Fatalf("expected id mismatch: %v", err)
	}
}

func TestNDJSONAndLengthPrefixAreBounded(t *testing.T) {
	call := NewCall("c1", "healthz")
	line, err := ToNDJSON(call)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasSuffix(line, "\n") {
		t.Fatal("missing terminator")
	}
	if _, err := CallFromNDJSON([]byte(line + "{}\n")); err == nil {
		t.Fatal("accepted multiple lines")
	}
	framed, err := EncodeLengthPrefixed(call)
	if err != nil {
		t.Fatal(err)
	}
	partial := append(append([]byte{}, framed...), 0, 0, 0)
	frames, rest, err := SplitLengthPrefixed(partial)
	if err != nil {
		t.Fatal(err)
	}
	if len(frames) != 1 || len(rest) != 3 {
		t.Fatalf("frames=%d rest=%d", len(frames), len(rest))
	}
	if _, _, err := SplitLengthPrefixed([]byte{0xff, 0xff, 0xff, 0xff}); err == nil {
		t.Fatal("accepted huge prefix")
	}
}

func TestCorrelatorIsConcurrentAndUnique(t *testing.T) {
	const count = 128
	correlator, err := NewCorrelator("rpc-")
	if err != nil {
		t.Fatal(err)
	}
	out := make(chan string, count)
	var group sync.WaitGroup
	for range count {
		group.Add(1)
		go func() {
			defer group.Done()
			value, err := correlator.Take()
			if err != nil {
				t.Errorf("Take: %v", err)
				return
			}
			out <- value
		}()
	}
	group.Wait()
	close(out)
	seen := map[string]struct{}{}
	for value := range out {
		if _, found := seen[value]; found {
			t.Fatalf("duplicate %s", value)
		}
		seen[value] = struct{}{}
	}
	if len(seen) != count {
		t.Fatalf("got %d", len(seen))
	}
	if _, err := NewCorrelator(strings.Repeat("x", 128)); err == nil {
		t.Fatal("accepted an unbounded correlation prefix")
	}
}

