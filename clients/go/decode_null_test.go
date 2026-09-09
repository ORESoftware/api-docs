package oresapidocs

import (
	"encoding/json"
	"fmt"
	"testing"
)

func TestDecodeReceiptRejectsNullOK(t *testing.T) {
	for _, status := range []string{"", `,"status":500`} {
		for _, value := range []string{"null", " \nnull\t "} {
			payload := fmt.Sprintf(`{"v":1,"op":"receipt","id":"null-ok","key":"healthz","ok":%s,"error":{}%s}`, value, status)
			if _, err := DecodeReceipt([]byte(payload)); err == nil {
				t.Errorf("accepted non-boolean ok in %s", payload)
			}
		}
	}
}

func TestTypedFrameMembersRejectExplicitNull(t *testing.T) {
	for _, kind := range []string{"call", "receipt"} {
		fields := []string{"v", "op", "id", "key", "transport", "traceId", "spanId"}
		if kind == "receipt" {
			fields = append(fields, "ok", "status")
		}
		for _, field := range fields {
			t.Run(kind+"/"+field, func(t *testing.T) {
				frame := map[string]any{"v": 1, "op": kind, "id": "null-field", "key": "healthz"}
				if kind == "receipt" {
					frame["ok"] = false
					frame["error"] = map[string]any{}
				}
				frame[field] = nil
				payload, err := json.Marshal(frame)
				if err != nil {
					t.Fatal(err)
				}
				if kind == "call" {
					_, err = DecodeCall(payload)
				} else {
					_, err = DecodeReceipt(payload)
				}
				if err == nil {
					t.Fatalf("accepted null %s", field)
				}
			})
		}
	}
}

func TestNullBodyAndBooleanFalseRemainValid(t *testing.T) {
	call, err := DecodeCall([]byte(`{"v":1,"op":"call","id":"body","key":"healthz","body":null}`))
	if err != nil || !call.Body.Present || string(call.Body.Value) != "null" {
		t.Fatalf("nullable call body changed: %v", err)
	}
	receipt, err := DecodeReceipt([]byte(`{"v":1,"op":"receipt","id":"body","key":"healthz","ok":true,"body":null}`))
	if err != nil || !receipt.Body.Present || string(receipt.Body.Value) != "null" {
		t.Fatalf("nullable receipt body changed: %v", err)
	}
	failure, err := DecodeReceipt([]byte(`{"v":1,"op":"receipt","id":"failure","key":"healthz","ok":false,"error":{}}`))
	if err != nil || failure.OK || failure.Status != nil {
		t.Fatalf("boolean false or optional status changed: %v", err)
	}
}
