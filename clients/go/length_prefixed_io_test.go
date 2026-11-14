package oresapidocs

import (
	"bytes"
	"encoding/binary"
	"errors"
	"io"
	"strings"
	"testing"
)

type byteAtATimeWriter struct{ bytes.Buffer }

func (w *byteAtATimeWriter) Write(p []byte) (int, error) { return w.Buffer.Write(p[:1]) }

type stalledWriter struct{}

func (stalledWriter) Write([]byte) (int, error) { return 0, nil }

func TestLengthPrefixedStreamIO(t *testing.T) {
	call := NewCall("c1", "healthz")
	receipt := SuccessReceipt("c1", "healthz", 200, OptionalJSON{})
	var output bytes.Buffer
	if err := WriteLengthPrefixed(&output, call); err != nil {
		t.Fatal(err)
	}
	if err := WriteLengthPrefixed(&output, receipt); err != nil {
		t.Fatal(err)
	}
	reader := bytes.NewReader(output.Bytes())
	gotCall, err := ReadCallLengthPrefixed(reader)
	if err != nil || gotCall.ID != call.ID || gotCall.Key != call.Key {
		t.Fatalf("call=%+v error=%v", gotCall, err)
	}
	gotReceipt, err := ReadReceiptLengthPrefixed(reader)
	if err != nil || gotReceipt.ID != receipt.ID || !gotReceipt.OK {
		t.Fatalf("receipt=%+v error=%v", gotReceipt, err)
	}
	if _, err := ReadLengthPrefixed(reader); !errors.Is(err, io.EOF) {
		t.Fatalf("expected clean EOF between frames, got %v", err)
	}
}

func TestLengthPrefixedReaderFailsClosed(t *testing.T) {
	tests := []struct {
		name    string
		data    []byte
		wantErr error
	}{
		{"partial prefix", []byte{0, 0}, io.ErrUnexpectedEOF},
		{"empty frame", []byte{0, 0, 0, 0}, nil},
		{"oversized frame", []byte{0xff, 0xff, 0xff, 0xff}, nil},
		{"truncated payload", []byte{0, 0, 0, 4, '{', '}'}, io.ErrUnexpectedEOF},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			_, err := ReadLengthPrefixed(bytes.NewReader(tc.data))
			if err == nil || (tc.wantErr != nil && !errors.Is(err, tc.wantErr)) {
				t.Fatalf("expected %v error, got %v", tc.wantErr, err)
			}
		})
	}
	// The splitter is also safe on 32-bit Go, where unchecked uint32 -> int
	// conversion would overflow and permit a negative slice bound.
	for _, size := range []uint32{0, uint32(MaxFrameBytes) + 1, ^uint32(0)} {
		var prefix [4]byte
		binary.BigEndian.PutUint32(prefix[:], size)
		if _, _, err := SplitLengthPrefixed(prefix[:]); err == nil {
			t.Fatalf("accepted length %d", size)
		}
	}
}

func TestLengthPrefixedWriterHandlesShortWritesAndErrors(t *testing.T) {
	call := NewCall("c1", "healthz")
	var writer byteAtATimeWriter
	if err := WriteLengthPrefixed(&writer, call); err != nil {
		t.Fatal(err)
	}
	encoded, err := EncodeLengthPrefixed(call)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(encoded, writer.Bytes()) {
		t.Fatal("short writes changed the wire bytes")
	}
	if err := WriteLengthPrefixed(stalledWriter{}, call); !errors.Is(err, io.ErrShortWrite) {
		t.Fatalf("zero-progress writer error: %v", err)
	}
	var output bytes.Buffer
	if err := WriteLengthPrefixed(&output, Call{}); err == nil || output.Len() != 0 {
		t.Fatalf("invalid envelope must be rejected before writing: %v", err)
	}
	if _, err := ReadCallLengthPrefixed(bytes.NewReader(encoded[:len(encoded)-1])); !errors.Is(err, io.ErrUnexpectedEOF) {
		t.Fatalf("expected truncated call error, got %v", err)
	}
	// A valid length prefix is not enough: typed readers enforce the envelope.
	malformed := append([]byte{0, 0, 0, 2}, []byte("{}")...)
	if _, err := ReadReceiptLengthPrefixed(bytes.NewReader(malformed)); err == nil || strings.Contains(err.Error(), "unexpected EOF") {
		t.Fatalf("invalid receipt must be rejected by decoder: %v", err)
	}
}
