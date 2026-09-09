package main

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"
)

func TestAdapterUsesRealDecoderAndEncoder(t *testing.T) {
	request := `{"cases":[{"name":"call","kind":"call","encoded":"{\"v\":1,\"op\":\"call\",\"id\":\"x\",\"key\":\"healthz\",\"body\":null}"},{"name":"bad","kind":"receipt","encoded":"{\"v\":1,\"op\":\"receipt\",\"id\":\"x\",\"key\":\"healthz\",\"ok\":null,\"error\":{}}"}]}`
	var output bytes.Buffer
	if err := run(strings.NewReader(request), &output); err != nil {
		t.Fatal(err)
	}
	var report struct {
		Results []result `json:"results"`
	}
	if err := json.Unmarshal(output.Bytes(), &report); err != nil {
		t.Fatal(err)
	}
	if len(report.Results) != 2 || !report.Results[0].Accepted || report.Results[1].Accepted {
		t.Fatalf("unexpected verdicts: %s", output.String())
	}
	if report.Results[0].Encoded == nil || !strings.Contains(*report.Results[0].Encoded, `"body":null`) {
		t.Fatal("lost explicit null body")
	}
	if report.Results[1].Encoded != nil {
		t.Fatal("rejection carried encoded output")
	}
}

func TestAdapterRejectsMalformedControlInput(t *testing.T) {
	for _, input := range []string{
		``, `null`, `{}`, `{"cases":[]}`, `{"cases":null}`, `{"cases":[],"expected":true}`,
		`{"cases":[{"name":"x","kind":"unknown","encoded":"{}"}]}`,
		`{"cases":[{"name":"x","kind":"call"}]}`,
		`{"cases":[{"kind":"call","encoded":"{}"}]}`,
		`{"cases":[{"name":"x","kind":"call","encoded":"{}","expected":false}]}`,
		`{"cases":[{"name":"x","kind":"call","encoded":"{}"},{"name":"x","kind":"call","encoded":"{}"}]}`,
		`{"cases":[{"name":"x","kind":"call","encoded":"{}"}]} {}`,
	} {
		t.Run(input, func(t *testing.T) {
			var output bytes.Buffer
			if err := run(strings.NewReader(input), &output); err == nil {
				t.Fatal("accepted invalid control input")
			}
			if output.Len() != 0 {
				t.Fatal("published partial evidence")
			}
		})
	}
}
