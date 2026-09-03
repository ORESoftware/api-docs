package ridlruntime

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
	FrameVersion int `json:"frame_version"`
	Cases        []struct {
		Name         string         `json:"name"`
		Encoded      string         `json:"encoded"`
		TCPPrefixHex string         `json:"tcp_prefix_hex"`
		Object       map[string]any `json:"object"`
	} `json:"cases"`
}

func TestConformanceFixturesRoundTripByteForByte(t *testing.T) {
	fixturePath := filepath.Join("..", "..", "examples", "frames", "conformance.json")
	data, err := os.ReadFile(fixturePath)
	if err != nil {
		t.Fatal(err)
	}
	var fixtures fixtureDocument
	if err := json.Unmarshal(data, &fixtures); err != nil {
		t.Fatal(err)
	}
	if fixtures.FrameVersion != int(FrameVersion) {
		t.Fatalf("fixture version %d != runtime version %d", fixtures.FrameVersion, FrameVersion)
	}
	for _, testCase := range fixtures.Cases {
		t.Run(testCase.Name, func(t *testing.T) {
			frame, err := Decode([]byte(testCase.Encoded))
			if err != nil {
				t.Fatal(err)
			}
			encoded, err := frame.Encode()
			if err != nil {
				t.Fatal(err)
			}
			if string(encoded) != testCase.Encoded {
				t.Fatalf("\nwant %s\n got %s", testCase.Encoded, encoded)
			}
			tcp, err := frame.EncodeTCP()
			if err != nil {
				t.Fatal(err)
			}
			if got := hex.EncodeToString(tcp[:LengthPrefixBytes]); got != testCase.TCPPrefixHex {
				t.Fatalf("prefix %s != %s", got, testCase.TCPPrefixHex)
			}
		})
	}
}

func TestBodyPresenceAndCanonicalCompaction(t *testing.T) {
	frame := DataFrame("1", json.RawMessage(` { "text" : "café — ok" } `))
	encoded, err := frame.Encode()
	if err != nil {
		t.Fatal(err)
	}
	want := `{"v":1,"id":"1","t":"data","body":{"text":"café — ok"}}`
	if string(encoded) != want {
		t.Fatalf("\nwant %s\n got %s", want, encoded)
	}

	nullFrame, err := Decode([]byte(`{"v":1,"id":"1","t":"data","body":null}`))
	if err != nil {
		t.Fatal(err)
	}
	if !nullFrame.HasBody || string(nullFrame.Body) != "null" {
		t.Fatalf("null body was collapsed: %#v", nullFrame)
	}
	absent, err := Decode([]byte(`{"v":1,"id":"1","t":"end"}`))
	if err != nil {
		t.Fatal(err)
	}
	if absent.HasBody {
		t.Fatal("absent body became present")
	}
}

func TestStrictDecodeAndBoundedStream(t *testing.T) {
	for _, payload := range [][]byte{
		[]byte(`{"v":1,"id":"1","t":"end","deadline":"5s"}`),
		[]byte(`{"v":257,"id":"1","t":"end"}`),
		[]byte(`{"v":1,"id":"1","t":"data"}`),
		[]byte(`{"v":1,"id":"1","t":"end","key":"healthz"}`),
		{0xff, 0xfe},
	} {
		if _, err := Decode(payload); err == nil {
			t.Fatalf("accepted invalid payload %q", payload)
		}
	}

	huge := []byte{0xff, 0xff, 0xff, 0xff}
	if _, _, err := DecodeStream(huge); err == nil || !strings.Contains(err.Error(), "over the") {
		t.Fatalf("huge length was not rejected: %v", err)
	}

	whole, err := EndFrame("1").EncodeTCP()
	if err != nil {
		t.Fatal(err)
	}
	partial, err := CancelFrame("2").EncodeTCP()
	if err != nil {
		t.Fatal(err)
	}
	buffer := append(append([]byte{}, whole...), partial[:3]...)
	frames, rest, err := DecodeStream(buffer)
	if err != nil {
		t.Fatal(err)
	}
	if len(frames) != 1 || len(rest) != 3 {
		t.Fatalf("frames=%d rest=%d", len(frames), len(rest))
	}
}

func TestMetaOrderAndConcurrentCorrelation(t *testing.T) {
	frame := EndFrame("1").WithMeta("z", "last").WithMeta("a", "first")
	encoded, err := frame.Encode()
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasSuffix(string(encoded), `,"meta":{"a":"first","z":"last"}}`) {
		t.Fatalf("meta is not canonical: %s", encoded)
	}

	correlator := NewCorrelator("c7-")
	const count = 128
	values := make(chan string, count)
	var group sync.WaitGroup
	for range count {
		group.Add(1)
		go func() {
			defer group.Done()
			values <- correlator.Take()
		}()
	}
	group.Wait()
	close(values)
	seen := map[string]struct{}{}
	for value := range values {
		if _, duplicate := seen[value]; duplicate {
			t.Fatalf("duplicate correlation id %s", value)
		}
		seen[value] = struct{}{}
	}
	if len(seen) != count {
		t.Fatalf("got %d correlation ids", len(seen))
	}
}
