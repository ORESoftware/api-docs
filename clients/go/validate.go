package oresapidocs

import (
	"encoding/json"
	"errors"
	"fmt"
	"regexp"
	"unicode/utf8"
)

var keyPattern = regexp.MustCompile(`^[A-Za-z][A-Za-z0-9_]*$`)
var headerNamePattern = regexp.MustCompile(`^[!#$%&'*+.^_` + "`" + `|~0-9a-z-]+$`)
var callFields = map[string]struct{}{"v": {}, "op": {}, "id": {}, "key": {}, "transport": {}, "path": {}, "query": {}, "headers": {}, "body": {}, "traceId": {}, "spanId": {}}
var receiptFields = map[string]struct{}{"v": {}, "op": {}, "id": {}, "key": {}, "transport": {}, "ok": {}, "status": {}, "body": {}, "error": {}, "traceId": {}, "spanId": {}}

func validTransport(value Transport) bool {
	return value == HTTP || value == TCP || value == WebSocket || value == NATS
}
func validString(value string, max int) bool {
	return value != "" && utf8.ValidString(value) && utf8.RuneCountInString(value) <= max
}
func validateObject(raw OptionalJSON, name string) error {
	if !raw.Present {
		return nil
	}
	if len(raw.Value) == 0 || !json.Valid(raw.Value) {
		return fmt.Errorf("%s must contain valid JSON", name)
	}
	var object map[string]json.RawMessage
	if err := json.Unmarshal(raw.Value, &object); err != nil || object == nil {
		return fmt.Errorf("%s must be a JSON object", name)
	}
	return nil
}
func validateHeaders(raw OptionalJSON) error {
	if err := validateObject(raw, "headers"); err != nil || !raw.Present {
		return err
	}
	var values map[string]json.RawMessage
	if err := json.Unmarshal(raw.Value, &values); err != nil {
		return errors.New("headers must be a JSON object")
	}
	for name, value := range values {
		if len(name) > 128 || !headerNamePattern.MatchString(name) {
			return fmt.Errorf("header name %q must be a canonical lowercase HTTP field name", name)
		}
		if !json.Valid(value) {
			return fmt.Errorf("headers.%s must contain valid JSON", name)
		}
	}
	return nil
}
func validateValue(raw OptionalJSON, name string) error {
	if !raw.Present {
		return nil
	}
	if len(raw.Value) == 0 || !json.Valid(raw.Value) {
		return fmt.Errorf("%s must contain one valid JSON value", name)
	}
	return nil
}
func validateCommon(version uint8, id, key string, transport *Transport, traceID, spanID *string) error {
	if version != RPCVersion {
		return fmt.Errorf("unsupported RPC version %d", version)
	}
	if !validString(id, 128) {
		return errors.New("id must be 1..128 characters")
	}
	if !keyPattern.MatchString(key) {
		return errors.New("key must be a portable RPC identifier")
	}
	if transport != nil && !validTransport(*transport) {
		return fmt.Errorf("unknown transport %q", *transport)
	}
	if traceID != nil && !validString(*traceID, 64) {
		return errors.New("traceId must be 1..64 characters")
	}
	if spanID != nil && !validString(*spanID, 32) {
		return errors.New("spanId must be 1..32 characters")
	}
	return nil
}
func (call Call) Validate() error {
	if err := validateCommon(call.Version, call.ID, call.Key, call.Transport, call.TraceID, call.SpanID); err != nil {
		return err
	}
	if err := validateObject(call.Path, "path"); err != nil {
		return err
	}
	if err := validateObject(call.Query, "query"); err != nil {
		return err
	}
	if err := validateHeaders(call.Headers); err != nil {
		return err
	}
	return validateValue(call.Body, "body")
}
func (receipt Receipt) Validate() error {
	if err := validateCommon(receipt.Version, receipt.ID, receipt.Key, receipt.Transport, receipt.TraceID, receipt.SpanID); err != nil {
		return err
	}
	if receipt.Status != nil && (*receipt.Status < 100 || *receipt.Status > 599) {
		return errors.New("status must be 100..599")
	}
	if err := validateValue(receipt.Body, "body"); err != nil {
		return err
	}
	if err := validateObject(receipt.Error, "error"); err != nil {
		return err
	}
	if receipt.OK {
		if receipt.Error.Present {
			return errors.New("a successful receipt must not carry error")
		}
		if receipt.Status != nil && (*receipt.Status < 200 || *receipt.Status > 399) {
			return errors.New("a successful receipt status must be 200..399")
		}
	} else {
		if receipt.Body.Present {
			return errors.New("an error receipt must not carry body")
		}
		if !receipt.Error.Present {
			return errors.New("an error receipt needs error")
		}
		if receipt.Status != nil && (*receipt.Status < 400 || *receipt.Status > 599) {
			return errors.New("an error receipt status must be 400..599")
		}
	}
	return nil
}
