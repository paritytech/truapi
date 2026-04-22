# RFC-0009: Unauthenticated Product Access

|                 |                                                                                          |
| --------------- | ---------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-04-13                                                                               |
| **Description** | Define how products behave before user login and how login is triggered                  |
| **Authors**     | Filippo Vecchiato                                                                        |

## Summary

This RFC formalizes the product lifecycle when no user is logged in. Products must be loadable and renderable in a read-only state without an authenticated user. A new `host_request_login` API call allows products to explicitly trigger the host login flow. The existing `host_account_connection_status_subscribe` subscription serves as the mechanism for products to react to authentication state changes.

## Motivation

Today the Host API does not specify what happens when a product is loaded but no user is signed in. Several questions are unresolved:

1. **Can a product render at all without a logged-in user?** Many products display public information (chain state, market data, governance proposals) that does not require authentication.
2. **Who initiates login?** It is unclear whether the host should auto-trigger login on the first key-dependent API call, or whether the product should explicitly request it.
3. **What happens to key-dependent API calls before login?** Products calling `host_account_get`, `host_sign_payload`, or `host_payment_balance_subscribe` before the user has authenticated will receive errors, but the expected error semantics are not defined.

Without clear guidance, product developers will implement inconsistent patterns — some blocking on login, others crashing on missing credentials, others showing blank screens.

### Design Intent

The intended behavior is:

- Allow the user to **view** the product in a read-only state.
- Allow the product to **trigger** login when the user takes an action that requires it.
- Products that use keys must provide a **meaningful read-only rendering** when no keys are available (i.e., the user is not logged in).

## Detailed Design

### Product Lifecycle States

A product operates in one of two authentication states:

```
┌──────────────┐    host_request_login()    ┌───────────────┐
│              │  ──────────────────────────▶│               │
│  Anonymous   │                             │ Authenticated │
│  (read-only) │  ◀──────────────────────────│               │
└──────────────┘    user disconnects         └───────────────┘
```

- **Anonymous**: No user is signed in. The product has access to public, non-identity APIs only.
- **Authenticated**: A user is signed in. The product has full access to the Host API surface (subject to permissions).

### API Availability by State

#### Always available (Anonymous + Authenticated)

These APIs do not depend on a user identity and must work regardless of login state:

| API Group          | Methods |
|--------------------|---------|
| General            | `host_handshake`, `host_feature_supported`, `host_navigate_to` |
| Storage            | `host_local_storage_read`, `host_local_storage_write`, `host_local_storage_clear` |
| Chain Interaction  | All `remote_chain_*` methods |
| Preimage           | `remote_preimage_lookup_subscribe` |
| Statement Store    | `remote_statement_store_subscribe` (read-only subscription) |
| Permissions        | `host_device_permission`, `remote_permission` |
| Account Status     | `host_account_connection_status_subscribe` |

#### Requires authentication

These APIs depend on a user identity. Calling them in the Anonymous state must return a well-defined error:

| API Group          | Methods | Error |
|--------------------|---------|-------|
| Accounts           | `host_account_get`, `host_account_get_alias`, `host_account_create_proof`, `host_get_non_product_accounts`, `host_get_user_id` | `NotConnected` |
| Signing            | `host_create_transaction`, `host_create_transaction_with_non_product_account`, `host_sign_raw`, `host_sign_payload` | `NotConnected` |
| Entropy            | `host_derive_entropy` | `NotConnected` |
| Payment            | `host_payment_balance_subscribe`, `host_payment_top_up`, `host_payment_request`, `host_payment_status_subscribe` | `NotConnected` |
| Statement Store    | `remote_statement_store_create_proof`, `remote_statement_store_submit` | `NotConnected` |
| Chat               | All `host_chat_*` methods | `NotConnected` |
| Notifications      | `host_push_notification` | `NotConnected` |

### New API: `host_request_login`

Products can explicitly ask the host to present the login UI to the user:

```rust
/// Request the host to present the login flow to the user. The host opens its
/// native sign-in UI (e.g. QR pairing). Returns once the flow
/// completes or the user rejects it.
///
/// Products should call this in response to a user action (e.g. tapping a
/// "Sign in" button rendered by the product), not automatically on load.
/// An optional human-readable reason displayed by the host in the login UI
/// (e.g. "Sign in to vote on Referendum #42"). The host may truncate or
/// ignore this string.
fn host_request_login(reason: Option<str>) -> Result<LoginResult, LoginErr>;

enum LoginResult {
    /// User successfully authenticated. The product will receive a
    /// `Connected` event via `host_account_connection_status_subscribe`.
    Success,
    /// User is already authenticated — no action was taken.
    /// This allows products to call `host_request_login` unconditionally
    /// without first checking connection status.
    AlreadyConnected,
    /// User dismissed/rejected the login UI without completing authentication.
    Rejected
}

enum LoginErr {
    Unknown(GenericErr)
}
```

### Behavioral Requirements

1. **Products must load without login.** The host must complete the handshake and make the product iframe/webview functional before any user is authenticated. Products must not assume a user is present at startup.

2. **Products must render meaningfully without keys.** If a product's primary function depends on user identity (e.g. a wallet, a voting app), it must still render a useful read-only view — for example, showing public chain data, general information, or a prominent "Sign in to continue" call to action. A blank screen or an error page is not acceptable.

3. **Login is product-initiated, not host-auto-triggered.** The host must not automatically present a login prompt when the product calls a key-dependent API. Instead, the host returns `NotConnected` and the product decides how to handle it — typically by showing a login prompt in its own UI that calls `host_request_login` when the user interacts with it. This keeps the user in control and avoids unexpected login popups during read-only browsing.

4. **`host_account_connection_status_subscribe` is the source of truth.** Products should subscribe to connection status on startup to learn the current state and react to changes. The subscription emits `Connected` after a successful login (whether triggered by the product via `host_request_login` or initiated by the user through the host's own UI) and `Disconnected` when the user signs out.

5. **No implicit login on permission/signing requests.** When a product calls `host_sign_payload` or similar while unauthenticated, the host must return `NotConnected` rather than chaining into a login flow. The rationale: combining login with a signing prompt creates a confusing UX where the user is asked to approve a transaction they haven't had time to review in context. Login and action authorization should be separate, deliberate steps.

### Product SDK Guidance

The Product SDK should provide ergonomic helpers for the common pattern:

```typescript
// Conceptual example — not a binding API proposal
const status = await hostApi.accountConnectionStatus.subscribe();

if (status === 'Disconnected') {
  // Render read-only UI with a login button
  renderReadOnlyView({
    onLoginClick: async () => {
      const result = await hostApi.requestLogin('Sign in to access your account');
      if (result === 'Rejected') {
        // User cancelled — stay in read-only mode
      }
      // On Success, the subscription will emit Connected
      // and the product re-renders with full capabilities
    }
  });
} else {
  // Render full authenticated UI
  renderAuthenticatedView();
}
```

## Alternatives

### Alternative A: Host auto-triggers login on first key-dependent call

The host intercepts calls like `host_account_get` or `host_sign_payload` and automatically presents the login UI before returning.

**Rejected because:**
- Creates unexpected popups during exploratory browsing.
- Conflates login with the action that required it (e.g. login + sign in one flow).
- Makes it impossible for the product to customize the login prompt or explain why login is needed.

### Alternative B: Products start only after login

The host does not load the product iframe until a user is authenticated.

**Rejected because:**
- Prevents read-only browsing of public content.
- Contradicts the design intent of allowing users to view products before committing to sign in.
- Poor UX for discovery — users cannot preview a product before creating an account.

### Alternative C: Login triggered by first permission or signing request

The host presents login when the product first requests a permission or signature.

**Rejected because:**
- Subset of Alternative A with the same problems.
- Additionally confusing: the user sees a login prompt when they expected a permission prompt.

## Unresolved Questions

1. **Product manifest declaration.** Should products declare in their manifest whether they support anonymous access? This could let the host skip loading products that require authentication, improving UX.
