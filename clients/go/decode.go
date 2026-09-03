package oresapidocs

import (
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode/utf8"
)

func DecodeCall(payload []byte) (Call, error) {
	raw, err := decodeObject(payload, callFields)
	if err != nil {
		return Call{}, err
	}
	var call Call
	if err = required(raw, "v", &call.Version); err != nil {
		return Call{}, err
	}
	var op string
	if err = required(raw, "op", &op); err != nil || op != "call" {
		return Call{}, errors.New("expected op call")
	}
	if err = required(raw, "id", &call.ID); err != nil {
		return Call{}, err
	}
	if err = required(raw, "key", &call.Key); err != nil {
		return Call{}, err
	}
	if err = optionalPtr(raw, "transport", &call.Transport); err != nil {
		return Call{}, err
	}
	call.Path = rawJSON(raw, "path")
	call.Query = rawJSON(raw, "query")
	call.Body = rawJSON(raw, "body")
	if err = optionalPtr(raw, "traceId", &call.TraceID); err != nil {
		return Call{}, err
	}
	if err = optionalPtr(raw, "spanId", &call.SpanID); err != nil {
		return Call{}, err
	}
	return call, call.Validate()
}
func DecodeReceipt(payload []byte) (Receipt, error) {
	raw, err := decodeObject(payload, receiptFields)
	if err != nil {
		return Receipt{}, err
	}
	var receipt Receipt
	if err = required(raw, "v", &receipt.Version); err != nil {
		return Receipt{}, err
	}
	var op string
	if err = required(raw, "op", &op); err != nil || op != "receipt" {
		return Receipt{}, errors.New("expected op receipt")
	}
	if err = required(raw, "id", &receipt.ID); err != nil {
		return Receipt{}, err
	}
	if err = required(raw, "key", &receipt.Key); err != nil {
		return Receipt{}, err
	}
	if err = optionalPtr(raw, "transport", &receipt.Transport); err != nil {
		return Receipt{}, err
	}
	if err = required(raw, "ok", &receipt.OK); err != nil {
		return Receipt{}, err
	}
	if err = optionalPtr(raw, "status", &receipt.Status); err != nil {
		return Receipt{}, err
	}
	receipt.Body = rawJSON(raw, "body")
	receipt.Error = rawJSON(raw, "error")
	if err = optionalPtr(raw, "traceId", &receipt.TraceID); err != nil {
		return Receipt{}, err
	}
	if err = optionalPtr(raw, "spanId", &receipt.SpanID); err != nil {
		return Receipt{}, err
	}
	return receipt, receipt.Validate()
}
func decodeObject(payload []byte, allowed map[string]struct{}) (map[string]json.RawMessage, error) {
	if len(payload) > MaxFrameBytes {
		return nil, fmt.Errorf("frame is %d bytes, over the %d limit", len(payload), MaxFrameBytes)
	}
	if !utf8.Valid(payload) {
		return nil, errors.New("frame is not UTF-8")
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(payload, &raw); err != nil {
		return nil, fmt.Errorf("frame is not JSON: %w", err)
	}
	if raw == nil {
		return nil, errors.New("frame must be a JSON object")
	}
	var unknown []string
	for key := range raw {
		if _, ok := allowed[key]; !ok {
			unknown = append(unknown, key)
		}
	}
	if len(unknown) > 0 {
		sort.Strings(unknown)
		return nil, fmt.Errorf("unknown frame member(s): %s", strings.Join(unknown, ", "))
	}
	return raw, nil
}
func required(raw map[string]json.RawMessage, name string, target any) error {
	value, ok := raw[name]
	if !ok {
		return fmt.Errorf("missing frame member %s", name)
	}
	if err := json.Unmarshal(value, target); err != nil {
		return fmt.Errorf("%s has the wrong type", name)
	}
	return nil
}
func optionalPtr[T any](raw map[string]json.RawMessage, name string, target **T) error {
	value, ok := raw[name]
	if !ok {
		return nil
	}
	var parsed T
	if err := json.Unmarshal(value, &parsed); err != nil {
		return fmt.Errorf("%s has the wrong type", name)
	}
	*target = &parsed
	return nil
}
func rawJSON(raw map[string]json.RawMessage, name string) OptionalJSON {
	value, ok := raw[name]
	return OptionalJSON{Present: ok, Value: clone(value)}
}
