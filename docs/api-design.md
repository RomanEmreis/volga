# API Design: Builder Naming

Volga's builder methods are named so that a reader can tell what kind of call a method is before
reading its docs. Follow these rules when adding a builder method, so the surface stays
predictable. A method that does not fit them is either renamed or listed here as an exception,
with the reason.

## The taxonomy

| Prefix | Meaning | Examples |
|---|---|---|
| `with_*` / `set_*` | Configure the application and its policies | `with_cors`, `set_tls`, `with_body_limit` |
| `use_*` | Activate configured functionality | `use_cors`, `use_open_api`, `use_static_files` |
| `map_*` | Register an endpoint, or a handler for an event | `map_get`, `map_fallback`, `map_err`, `map_problem` |
| `wrap` / `attach` | Extend the middleware pipeline | `wrap(\|ctx, next\| ...)`, `attach(middleware)` |
| purpose-named | An operation named after what it is for | `authorize`, `filter`, `tap_req`, `cors()`, `open_api` |

Whether a purpose-named operation is a policy or a middleware is an implementation detail: the
name says what it is for, not how it is built.

## On `App`, the prefix encodes the phase

Setting up an `App` has two phases, and the receiver of a method says which one it belongs to:

- **Building the server and defining policies** takes `self` by value and returns `Self`: every
  `with_*`, `set_*` and `without_*`. These chain off `App::new()`.
- **Changing the pipeline or the routes** takes `&mut self` and returns `&mut Self`: every
  `use_*` and `map_*`, `wrap` and `attach`, and the purpose-named operations.

```rust
let mut app = App::new() // build phase: `self` by value
    .with_cors(|cors| cors.with_any_origin())
    .with_body_limit(Limit::Limited(1024 * 1024));

app.use_cors(); // pipeline phase: `&mut self`
app.map_get("/health", || async { "ok" });
```

A method whose prefix and receiver disagree has the wrong name.

**Exception:** the middleware `with(|args, next| ...)` takes `&mut self`. It belongs to the
`wrap` / `attach` row - it is `wrap` with extractors in place of `HttpContext` - and stays as it
is. It is `with`, not `with_*`: no `with_*` on `App` takes `&mut self`.

Outside the prefixes: `new` and `with_shutdown` are constructors (`with_shutdown` returns the app
together with its `ShutdownHandle`); `bind` and `shutdown_on` belong to the build phase and take
`self`; `add_*` registers a service in the DI container and `group` opens a route group, both
with `&mut self`; `run*` start the server.

## On `Route` and `RouteGroup`, the name says where the policy comes from

A `Route` (returned by `map_*`) and a `RouteGroup` (lent to the closure of `group`) have no build
phase: every call on them changes the routes they cover. Their receivers follow from ownership -
a `Route` takes `self`, a `RouteGroup` takes `&mut self` - and mean nothing more. So the name
alone tells the kind of call:

| On Route / Group | When | Examples |
|---|---|---|
| `with_X` / `without_X` | The policy is defined in the call | `with_body_limit`, `without_body_limit`, `with_cache_control`, `with_compression`, `with_decompression` |
| `X()` / `X_with(name)` / `disable_X()` | The route selects a policy defined on `App` | `cors()`, `cors_with(name)`, `disable_cors()`, `fixed_window(key)` |
| `use_X` | Activates something configured on `App` | `RouteGroup::use_static_files` and `use_static_assets` (the app's `HostEnv`) |
| A verb | An action, not a policy | `authorize`, `filter`, `map_*`, `wrap`, `attach`, `tap_req` |

There is no `set_X` on a route or a group. Each `with_X` there defines the policy for its own
scope, so there is no config object to install.

`Route::open_api(|cfg| ...)` and `RouteGroup::open_api` are purpose-named. They describe the
routes for the generated document, not how the routes handle requests, so they are route
metadata rather than a policy and do not take `with_`.

## One name on every level when the question is the same

When `App`, `RouteGroup` and `Route` answer the same question, they use the same name: `map_err`,
`map_problem`, `map_ok`, `authorize`, `filter`, `tap_req`, `wrap`, `attach`, `with`,
`with_body_limit` / `without_body_limit`, `with_cache_control`. The receivers differ - by the
phase rule on `App`, by ownership on the other two - but the name does not.

Where the names differ, it is because the question differs. The allowed asymmetries:

- `app.use_compression()` next to `group.with_compression()` and `route.with_compression()`, and
  the same for decompression. On `App`, adding middleware to the global pipeline is the pipeline
  phase, so it is `use_*`. On a route or a group there is no phase, and the policy - compress with
  the defaults - is defined in the call, so it is `with_*`.
- `app.with_cors(..)` and `app.use_cors()` next to `route.cors()`, `cors_with(name)` and
  `disable_cors()`. The app defines the CORS policies and `use_cors` turns CORS on; a route or a
  group selects one of the policies the app defined, or none.
- The rate limiters work like CORS: `app.with_fixed_window(policy)` defines a policy,
  `app.use_fixed_window(key)` applies it to every request, and `route.fixed_window(key)` to one
  route.
- `app.with_bearer_auth(..)` configures how tokens are validated, which only the app can do;
  `authorize` exists on every level, since it is the same action wherever it is called.
- `app.with_open_api(..)` and `app.use_open_api()` configure and serve the document;
  `route.open_api(..)` describes one route in it.

## `with_*`

Configures, enriches, or composes behavior using the most natural input for that API. Takes
`self` by value.

This is the primary fluent configuration vocabulary.

```rust
// Closure form
let app = App::new().with_cors(|cors| cors.with_any_origin());

// Value form - fluent configuration step
let config = HstsConfig::default().with_max_age(Duration::from_secs(10));
```

`with_*` is used for builder-style configuration, including:

- Closures over a config object (`with_cors(|c| ...)`)
- Enabling flags (`with_preload()`)
- Attaching behavior or strategies (`with_fixed_window(policy)`)
- Setting field-like values that conceptually shape the configuration (`with_max_age(...)`)

Use `with_*` when the operation is part of fluent configuration.

Do not use `with_*(bool)` - split into `with_*` / `without_*` instead.

For installing a pre-built config object directly, use `set_*`.

## `set_*`

Installs a pre-built config object, or sets a single typed field. Takes `self` by value.

```rust
// Whole-config form - peer to `with_cors`
let cors = CorsConfig::default().with_any_origin();
let app = App::new().set_cors(cors);

// Single-field form
let auth = BearerAuthConfig::default().set_decoding_key(key);
```

**Every `with_X(closure)` on `App` that builds a config object has a `set_X(config)` peer**, so
a config built elsewhere - in a helper, in a test, shared between apps - goes in without
`with_X(|_| config)`. `set_X` installs the config whole and replaces whatever was there; `with_X`
hands the closure a config to refine. A `with_X` that takes the value itself, such as
`with_body_limit(limit)` or `with_fixed_window(policy)`, needs no peer.

The whole-config form of `set_*` exists alongside `with_*` as a deliberate peer - callers pick the
one that matches their authoring style. A single trait-based overload isn't feasible: Rust can't
infer closure parameter types when a trait has blanket impls for both `Self` and
`FnOnce(Self) -> Self`. Use whichever form reads better at the call site.

## `without_*`

Turns a flag off or clears a field. Zero-argument. Takes `self` by value.

```rust
let config = HstsConfig::default().with_preload().without_sub_domains();
```

Use `without_*` as the opposite of a boolean `with_*`. **Do not** write `with_foo(false)` - write
`without_foo()`.

## `with_default_*`

Rare. Use **only** when "default" requires non-trivial work beyond `Default::default()` - e.g.,
file discovery, env reads, convention-over-configuration behavior.

```rust
// Searches CWD for app_config.toml or .json, loads, panics if missing.
let app = App::new().with_default_config();
```

**Do not** add `with_default_cors()` as a shortcut for `set_cors(CorsConfig::default())` - the
latter is already explicit. If the only work is `Default::default()`, drop it.

## `use_*`

Activates functionality configured on `App` by registering middleware or routes. Takes
`&mut self`. Zero- or single-argument (for typed keys).

```rust
let mut app = App::new().with_cors(|c| c.with_any_origin());
app.use_cors(); // wires up CORS middleware
```

Use `use_*` when the method makes a visible change to the request pipeline (registers routes or
adds middleware). **Do not** use `use_*` for pure state configuration - that's what `with_*` /
`set_*` is for - or for registering a handler, which is `map_*`.

## `map_*`

Registers an endpoint (`map_get`, `map_ws`, `map_fallback`) or a handler for an event (`map_err`
for an error, `map_ok` for a successful response). `map_problem` is `map_err` with a handler of
volga's own, which renders errors as RFC 9457 problem details. On `App` it takes `&mut self`.

On every level, `map_err` and `map_problem` set the one error handler of that scope, so whichever
is called last handles the errors - see [Error handling](#error-handling):

```rust
app.map_err(|err: Error| async move { status!(err.status().as_u16(), "{err}") });
app.map_problem(); // replaces the handler above
```

## Error handling

`map_ok` and `map_err` divide what a request can end in between them, and they mean the same on
`App`, `RouteGroup` and `Route`:

- **`map_ok` sees the success path and explicit responses.** A response that a handler or a
  middleware returns is a response, whatever its status: `status!(401)` returned by a handler
  goes through `map_ok` and never reaches `map_err`. So does whatever a `map_fallback` of the
  application's own answers, and what a scoped `map_err` answers an error with, once it is
  outside that scope.
- **`map_err` sees the rejections**: an `Err` from a handler, an extractor or a middleware, and
  every rejection volga produces itself.

### A scope's handler covers the whole scope

`map_err` on a `Route` or a `RouteGroup` is the outermost layer of its scope, whatever order it
was registered in. It wraps the route's or the group's handler and every middleware of the
scope, the ones registered after it included, so the natural order works:

```rust
app.map_get("/admin", handler)
    .authorize::<Claims>(roles(["admin"]))
    .map_problem(); // answers the `401` of the `authorize` above it
```

- **One handler per scope.** A second `map_err`, or `map_problem`, on the same route or group
  replaces the first, as on `App`.
- **Returning the error passes it on**, like a rethrow from a nested `catch`: from the route to
  the groups around it, innermost first, and then to `App::map_err`. `impl IntoResponse for
  Error` is `Err(self)`, so `|err: Error| err` is a handler that passes everything on.
- **A scope sees its own errors only.** The middleware of a group around a route is outside the
  route's scope, so the route's handler does not see its errors. The errors of the global
  middleware and of the application fallback reach `App::map_err` alone.

Internally a scope's handler is kept apart from its middleware until the scope is closed - a
route's when a group around it applies its layers, or when the pipeline is composed; a group's
when its closure returns - and is then put in front of everything the scope holds.

### Framework rejections are `Err`

A rejection the framework produces itself is an `Err(Error)`, and what user code returns
explicitly goes out as it is:

| Rejection | Status | Error headers |
|---|---|---|
| `authorize` | `400`, `401`, `403`, `503` | `WWW-Authenticate`, `Cache-Control: no-store` |
| Rate limiting | `429` | - |
| Router, no route for the path (the default fallback) | `404` | - |
| Router, no route for the method | `405` | `Allow` |
| Request body over its limit | `413` | - |
| `filter` saying no | `400`, or the status of its `Err` | - |
| Extractors | the extractor's | - |
| Static files, denied or with no fallback file | `403`, `404` | - |
| Compression, no acceptable encoding | `406` | `Vary`, `Accept-Encoding` |

Responses that are not rejections stay `Ok`: a CORS preflight's `204`, a static file's `304`,
the HTTPS redirect and the answer to an implicit `HEAD`.

**Do not** answer a rejection with `Ok(status!(..))` in new code inside volga: `map_err` would
not see it, and `map_ok` would take it for a success.

### Error headers are applied over any handler

A protocol header belongs to the error rather than to one way of rendering it, so it is carried
by the error: `Error::with_header`. The framework adds the error's headers to whatever response
its error handler answers with - the default one, `map_problem` or one of the application's own,
such as `|e: Error| status!(500, ..)` - and a header the handler set itself wins. A handler that
answers with an error passes that error on, headers and all, so they are added once, where the
error is answered in the end.

Middleware that adds headers to every response adds them to an error the same way: CORS puts
its headers on an `Err` passing through it as error headers, so that a browser can read a `401`
or a `404` rather than report a blocked origin.

`Error::with_response` is a separate thing: it means "answer exactly this, unformatted". The
error's headers are added to that response too.

## Type aliases over a wrapper

An alias over a wrapper type is named after the wrapper followed by the name of the inner type,
so that it reads as the type it stands for:

```rust
pub type ValidJson<T> = Valid<Json<T>>;
pub type ValidNamedPath<T> = Valid<NamedPath<T>>;
```

An alias whose name points at a different type fails the same call-site test as a method with
the wrong prefix: `ValidPath<T>` promised a validated `Path<T>` and delivered a `NamedPath<T>`.
Where the wrapper cannot go around an inner type, there is no alias for it, rather than an alias
named after it.

## Deprecating a name

A rename keeps the old name as an alias that forwards to the new method:

```rust
/// Adds middleware that includes a configured `cache-control` header for all responses from this route.
#[deprecated(since = "0.14.1", note = "renamed to `with_cache_control`")]
pub fn cache_control<F>(self, config: F) -> Self
where
    F: Fn(CacheControl) -> CacheControl,
{
    self.with_cache_control(config)
}
```

The alias keeps its one-line doc and drops the example. Tests, examples and docs move to the new
name; one test per alias, under `#[allow(deprecated)]`, keeps the old name working.

A renamed type alias keeps the old name the same way, as a `#[deprecated]` alias of the new one.
Re-exporting it warns, so its `pub use` goes on a line of its own under `#[allow(deprecated)]`,
apart from the names that are not deprecated.

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

3. If the subsystem changes the pipeline, add a `use_my_feature(&mut self)` that activates it,
   and, where a route or a group can pick it, `my_feature()` / `my_feature_with(name)` /
   `disable_my_feature()` on `Route` and `RouteGroup`.
4. If `MyConfig` has boolean flags, prefer `with_flag` / `without_flag` zero-arg pairs over
   `with_flag(bool)`.
5. Add rustdoc examples showing both the closure and value forms of `with_my_config` /
   `set_my_config`.
