# Generated compile-time route objects

Do not edit these files. They are produced from `examples/*.route-map.json`:

```sh
python3 scripts/generate-routes.py
python3 scripts/generate-routes.py --check
```

TypeScript `Routes` keys, Rust `RouteKey` enums, and Dart `Routes.byKey` all
come from the same JSON so a missing handler fails to compile in each language.
