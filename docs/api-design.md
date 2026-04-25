# API Design: The Five Verbs

Volga's public builder API uses five verbs, each with one job. Follow this convention when adding new builder methods so the surface stays predictable.

## `with_*`

Configures, enriches, or composes behavior using the most natural input for that API. Takes `self` by value.

This is the primary fluent configuration vocabulary.

```rust
// Closure form — build from default
let app = App::new().with_cors(|cors| cors.with_any_origin());

// Value form — fluent configuration step
let config = HstsConfig::default().with_max_age(Duration::from_secs(10));
```

`with_*` is used for builder-style configuration, including:

- Closures over a default config (`with_cors(|c| ...)`)
- Enabling flags (`with_preload()`)
- Attaching behavior or strategies (with_token_bucket(bucket))
- Setting field-like values that conceptually shape the configuration (with_max_age(...))

Use `with_*` when the operation is part of fluent configuration.

Do not use `with_*(bool)` — split into `with_*` / `without_*` instead.

For installing a pre-built config object directly, use `set_*`.

## `set_*`

Installs a pre-built config object, or sets a single typed field. Takes `self` by value.

```rust
// Whole-config form — peer to `with_cors`
let cors = CorsConfig::default().with_any_origin();
let app = App::new().set_cors(cors);

// Single-field form
let auth = BearerAuthConfig::new().set_decoding_key(key);
```

The whole-config form of `set_*` exists alongside `with_*` as a deliberate peer — callers pick the one that matches their authoring style. A single trait-based overload isn't feasible: Rust can't infer closure parameter types when a trait has blanket impls for both `Self` and `FnOnce(Self) -> Self`. Use whichever form reads better at the call site.

## `without_*`

Turns a flag off or clears a field. Zero-argument. Takes `self` by value.

```rust
let config = HstsConfig::default().with_preload().without_sub_domains();
```

Use `without_*` as the opposite of a boolean `with_*`. **Do not** write `with_foo(false)` — write `without_foo()`.

## `with_default_*`

Rare. Use **only** when "default" requires non-trivial work beyond `Default::default()` — e.g., file discovery, env reads, convention-over-configuration behavior.

```rust
// Searches CWD for app_config.toml or .json, loads, panics if missing.
let app = App::new().with_default_config();
```

**Do not** add `with_default_cors()` as a shortcut for `set_cors(CorsConfig::default())` — the latter is already explicit. If the only work is `Default::default()`, drop it.

## `use_*`

Enables a feature that registers middleware or special routes. Takes `&mut self`. Zero- or single-argument (for typed keys).

```rust
let mut app = App::new().with_cors(|c| c.with_any_origin());
app.use_cors(); // wires up CORS middleware
```

Use `use_*` when the method makes a visible change to the request pipeline (registers routes, adds middleware, turns on a map-err handler). **Do not** use `use_*` for pure state configuration — that's what `with_*` / `set_*` is for.

## Adding a New Config Subsystem

1. Define the config struct: `pub struct MyConfig { ... }` with `Default`.
2. Add a pair of `App` builder methods:

```rust
impl App {
    pub fn with_my_config<F>(self, f: F) -> Self
    where
        F: FnOnce(MyConfig) -> MyConfig,
    {
        self.set_my_config(f(MyConfig::default()))
    }

    pub fn set_my_config(mut self, config: MyConfig) -> Self {
        self.my_config = Some(config);
        self
    }
}
```

3. If `MyConfig` has boolean flags, prefer `with_flag` / `without_flag` zero-arg pairs over `with_flag(bool)`.
4. Add rustdoc examples showing both the closure and value forms of `with_my_config` / `set_my_config`.
