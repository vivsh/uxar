# Evaluation: Replace route.rs with bundlepart.rs Approach

## Executive Summary

**Recommendation: NOT YET - Needs Runtime API Alignment**

The bundlepart.rs approach is architecturally superior and ready for use, but we need to align the runtime API first. Current route.rs generates `ViewMeta` which doesn't match `bundles::route(handler, RouteConf)`.

## Current State Analysis

### Current route.rs (547 lines)

- **Generates:** `BundlePart::Route(method_router, ViewMeta)`
- **Features:**
  - `name` override
  - `methods` (multiple HTTP methods)
  - `path` with validation
  - `summary` (from attr or first doc line)
  - `description` (from attr or remaining docs)
  - `tags` (multiple)
  - `param` overrides (name, type, source, hide with `_`)
  - `response` overrides (status codes, types)
  - Separate entry points for functions vs methods
  - Manual arg/return patching logic

### Proposed bundlepart.rs approach (~130 lines)

- **Generates:** `bundles::route(handler, RouteConf).patch(PatchOp)`
- **Features:**
  - Clean separation: conf vs spec overrides
  - Automatic `PatchOp` chain generation
  - Single entry point for fn/methods
  - All functions ≤50 lines
  - Better error messages with spans
  - Testable, maintainable architecture

## Gap Analysis

### ✅ Already Supported

1. **Method validation** - Both validate HTTP methods
2. **Path validation** - Both validate path format
3. **Default values** - Both provide sensible defaults (GET, "/{name}")
4. **Function/method support** - bundlepart.rs handles both automatically
5. **Arg name extraction** - bundlepart.rs via `fnspec.rs`
6. **Return type info** - bundlepart.rs via `PatchOp.append()`

### ⚠️ Needs Implementation

1. **Summary extraction from docs** - Current route.rs splits first line as summary
   - **Solution:** bundlepart.rs can use `spec.docs` and split in builder
2. **Param source detection** - Current route.rs detects `Path`, `Query`, etc.
   - **Solution:** bundlepart.rs `ArgOverride` has `source` field (not implemented yet)
3. **Param hiding** - Current route.rs allows `param(name = "_")` to hide params
   - **Solution:** Add to `ArgOverride` model and patch generation

4. **Multiple response status codes** - Current route.rs has `response(status = 404)`
   - **Solution:** bundlepart.rs `ReturnOverride` already has this

### ❌ API Mismatch (BLOCKER)

**Current route.rs generates:**

```rust
BundlePart::Route(method_router, ViewMeta)
// where ViewMeta has: name, methods, path, summary, description, tags, params, responses
```

**But bundlepart.rs expects:**

```rust
bundles::route(handler, RouteConf).patch(...)
// where RouteConf has: name, methods, path
// and patch adds: description, args, returns
```

**Root cause:** The current runtime expects `ViewMeta` which bundles params/responses, but `bundles::route()` uses `RouteConf` + reflection + patching.

## Migration Path

### Phase 1: Runtime API Alignment (Required First)

1. **Verify `bundles::route` signature:**

   ```rust
   pub fn route<H, T, Args>(handler: H, meta: RouteConf) -> BundlePart
   where
       H: Handler<T, Site> + Specable<Args>,
       Args: IntoArgSpecs,
   ```

   ✅ Confirmed - takes `RouteConf`, does runtime reflection

2. **Remove deprecated `ViewMeta`:**
   - Current route.rs generates `ViewMeta`
   - Runtime should only use `Operation` (from reflection + patches)
   - **Action:** Check if `ViewMeta` is still used anywhere

3. **Verify `PatchOp` API matches:**
   - ✅ `.description()` - function description
   - ✅ `.arg(pos).name().doc().done()` - arg metadata
   - ✅ `.append().typed::<T>().status().doc().done()` - returns
   - Need: `.arg().source()` for path/query/body detection

### Phase 2: Implement Missing Features

1. **Add to `ArgOverride` in bundlepart.rs:**

   ```rust
   pub struct ArgOverride {
       pub pos: Option<usize>,
       pub name: String,
       pub ty: Option<String>,
       pub description: Option<String>,
       pub source: Option<String>,  // NEW: "path", "query", "body", "header"
       pub hidden: bool,              // NEW: hide from docs
       pub span: Span,
   }
   ```

2. **Update `build_arg_patch` to use source:**

   ```rust
   if let Some(source) = &o.source {
       patch = quote! { #patch.source(#source) };
   }
   if o.hidden {
       patch = quote! { #patch.hidden(true) };
   }
   ```

3. **Summary extraction in builder:**
   ```rust
   fn build_route_conf(...) -> Result<...> {
       let description = meta.description.as_ref()
           .or_else(|| extract_summary_from_docs(&spec.docs));
       // ... use in patch generation
   }
   ```

### Phase 3: Testing

1. **Unit tests:**
   - Conf builder with various inputs
   - Validation edge cases
   - Patch generation correctness

2. **Integration tests:**
   - Compare generated code vs old route.rs
   - Ensure runtime behavior identical

3. **Migration tests:**
   - Test all existing route macros still work
   - No breaking changes to user code

### Phase 4: Migration

1. **Rename old route.rs:** `route_legacy.rs`
2. **Create new route.rs** using bundlepart.rs approach
3. **Deprecate legacy** with clear migration guide
4. **Remove legacy** after one release cycle

## Risks & Mitigation

### Risk 1: Runtime Incompatibility

**Risk:** `bundles::route` doesn't actually use `RouteConf` as shown
**Mitigation:** We verified it does - line 328 of bundles.rs confirms signature
**Status:** ✅ Verified

### Risk 2: Missing ViewMeta Features

**Risk:** `ViewMeta` has features not in `RouteConf` + `Operation`
**Mitigation:** Need to audit if `ViewMeta` is used by anything else
**Action:** Search for `ViewMeta` usage outside route.rs
**Status:** ⚠️ NOT FOUND - might be dead code

### Risk 3: Breaking User Code

**Risk:** Different attribute syntax breaks existing routes
**Mitigation:** Keep same syntax, just different codegen
**Example:**

```rust
// Old and new both support:
#[route(method = "GET", url = "/users", tag = "admin")]
async fn list_users() -> Json<Vec<User>> { ... }
```

**Status:** ✅ No breaking changes needed

### Risk 4: Param Extraction Complexity

**Risk:** Extracting `Path`, `Query` detection is complex
**Mitigation:** Can defer - runtime reflection already does this
**Note:** Current route.rs uses `IntoApiParts` - same for bundlepart
**Status:** ✅ Not a blocker - runtime handles it

## Recommendations

### Immediate Actions

1. ✅ **Verify `ViewMeta` is deprecated**
   - Search entire codebase for `ViewMeta` usage
   - Confirm it's not used by bundles.rs or anywhere else

2. ⚠️ **Add missing `ArgOverride` fields**
   - `source: Option<String>`
   - `hidden: bool`

3. ⚠️ **Implement summary extraction**
   - Split first line of docs as summary
   - Use in patch generation

4. ✅ **Write comprehensive tests**
   - Unit tests for new route.rs
   - Integration tests comparing old vs new

### Decision Point

**Can we migrate now?**

- ✅ Architecture is sound
- ✅ Runtime API matches (RouteConf + PatchOp)
- ⚠️ Need to add `source` and `hidden` to `ArgOverride`
- ⚠️ Need to implement summary extraction
- ❓ Need to confirm `ViewMeta` is dead code

**Timeline:**

- Phase 1 (Verification): 1-2 hours
- Phase 2 (Implementation): 4-6 hours
- Phase 3 (Testing): 4-6 hours
- Phase 4 (Migration): 2-3 hours

**Total: ~15 hours** to full migration

## Conclusion

The bundlepart.rs approach is **architecturally superior** and **ready for use**, but requires:

1. Confirmation that `ViewMeta` is deprecated (couldn't find usage - likely dead code)
2. Small additions to `ArgOverride` (source, hidden fields)
3. Summary extraction logic
4. Comprehensive testing

**Recommendation:** Proceed with migration after completing Phase 1 verification (2 hours) and Phase 2 implementation (6 hours). The investment is worth it for long-term maintainability.

The new approach will reduce route.rs from 547 lines to ~130 lines while improving:

- Testability (pure functions)
- Maintainability (clear separation)
- Correctness (better validation)
- Consistency (shared with task, cron, flow, etc.)
