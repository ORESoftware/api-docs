package oresapidocs

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
)

// ReadLengthPrefixed reads exactly one bounded v1 TCP payload from reader.
// It does not consume bytes from the next frame. Call DecodeCall or
// DecodeReceipt on the returned payload to enforce the v1 envelope contract.
func ReadLengthPrefixed(reader io.Reader) ([]byte, error) {
	var prefix [LengthPrefixBytes]byte
	if _, err := io.ReadFull(reader, prefix[:]); err != nil {
		return nil, fmt.Errorf("read RPC frame length: %w", err)
	}
	length := binary.BigEndian.Uint32(prefix[:])
	if length == 0 || length > uint32(MaxFrameBytes) {
		return nil, fmt.Errorf("declared RPC frame length %d is outside 1..%d", length, MaxFrameBytes)
	}
	payload := make([]byte, int(length))
	if _, err := io.ReadFull(reader, payload); err != nil {
		return nil, fmt.Errorf("read RPC frame payload: %w", err)
	}
	return payload, nil
}

// ReadCallLengthPrefixed reads and validates one v1 call from a TCP stream.
func ReadCallLengthPrefixed(reader io.Reader) (Call, error) {
	payload, err := ReadLengthPrefixed(reader)
	if err != nil {
		return Call{}, err
	}
	return DecodeCall(payload)
}

// ReadReceiptLengthPrefixed reads and validates one v1 receipt from a TCP stream.
func ReadReceiptLengthPrefixed(reader io.Reader) (Receipt, error) {
	payload, err := ReadLengthPrefixed(reader)
	if err != nil {
		return Receipt{}, err
	}
	return DecodeReceipt(payload)
}

// WriteLengthPrefixed encodes and writes one bounded v1 frame. It completes
// short writes and reports a writer that makes no progress.
func WriteLengthPrefixed(writer io.Writer, value interface{ Encode() ([]byte, error) }) error {
	frame, err := EncodeLengthPrefixed(value)
	if err != nil {
		return err
	}
	for len(frame) > 0 {
		n, err := writer.Write(frame)
		if n < 0 || n > len(frame) {
			return errors.New("RPC writer returned an invalid byte count")
		}
		frame = frame[n:]
		if err != nil {
			return err
		}
		if n == 0 {
			return io.ErrShortWrite
		}
	}
	return nil
}
