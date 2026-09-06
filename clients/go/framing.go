package oresapidocs

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
)

func ToNDJSON(value interface{ Encode() ([]byte, error) }) (string, error) {
	payload, err := value.Encode()
	if err != nil {
		return "", err
	}
	return string(payload) + "\n", nil
}
func CallFromNDJSON(line []byte) (Call, error) {
	payload, err := oneLine(line)
	if err != nil {
		return Call{}, err
	}
	return DecodeCall(payload)
}
func ReceiptFromNDJSON(line []byte) (Receipt, error) {
	payload, err := oneLine(line)
	if err != nil {
		return Receipt{}, err
	}
	return DecodeReceipt(payload)
}
func oneLine(line []byte) ([]byte, error) {
	if len(line) > MaxFrameBytes+2 {
		return nil, errors.New("NDJSON frame exceeds limit")
	}
	line = bytes.TrimSuffix(line, []byte("\r\n"))
	line = bytes.TrimSuffix(line, []byte("\n"))
	if len(line) == 0 {
		return nil, errors.New("NDJSON input is empty")
	}
	if bytes.ContainsAny(line, "\r\n") {
		return nil, errors.New("NDJSON input must contain exactly one JSON object")
	}
	return line, nil
}
func EncodeLengthPrefixed(value interface{ Encode() ([]byte, error) }) ([]byte, error) {
	payload, err := value.Encode()
	if err != nil {
		return nil, err
	}
	out := make([]byte, LengthPrefixBytes+len(payload))
	binary.BigEndian.PutUint32(out[:LengthPrefixBytes], uint32(len(payload)))
	copy(out[LengthPrefixBytes:], payload)
	return out, nil
}
func SplitLengthPrefixed(buffer []byte) ([][]byte, []byte, error) {
	var frames [][]byte
	offset := 0
	for len(buffer)-offset >= LengthPrefixBytes {
		length := int(binary.BigEndian.Uint32(buffer[offset : offset+LengthPrefixBytes]))
		if length > MaxFrameBytes {
			return nil, nil, fmt.Errorf("declared frame length %d is over the %d limit", length, MaxFrameBytes)
		}
		start := offset + LengthPrefixBytes
		if len(buffer)-start < length {
			break
		}
		frames = append(frames, clone(buffer[start:start+length]))
		offset = start + length
	}
	return frames, clone(buffer[offset:]), nil
}
func AssertReceiptForCall(call Call, receipt Receipt) error {
	if err := call.Validate(); err != nil {
		return err
	}
	if err := receipt.Validate(); err != nil {
		return err
	}
	if call.ID != receipt.ID {
		return errors.New("receipt id does not match call id")
	}
	if call.Key != receipt.Key {
		return errors.New("receipt key does not match call key")
	}
	if call.Transport != nil && receipt.Transport != nil && *call.Transport != *receipt.Transport {
		return errors.New("receipt transport does not match call transport")
	}
	return nil
}
