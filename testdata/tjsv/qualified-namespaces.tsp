// Consumer regression for ORESoftware/typespec-json-schema-validator namespace identity.
// This is test evidence, not a domain authority. The real request-surface TypeSpec and JSON
// Schema remain the independently authored authorities exercised by request-surface-contracts.
namespace Demo {
  model OuterModel {
    id: string;
  }

  // Already-qualified nested namespaces must not be prefixed with Demo again.
  namespace Demo.Inner {
    model WeatherReading {
      celsius: float64;
    }

    namespace Demo.Inner.Deep {
      model DeepReading {
        value: int32;
      }
    }
  }

  // Genuinely relative namespaces still extend the active namespace.
  namespace Relative {
    model LocalReading {
      ok: boolean;
    }
  }

  // Prefix detection must be segment-aware: Demo2 is relative, not already under Demo.
  namespace Demo2 {
    model PrefixCollision {
      label: string;
    }
  }
}
