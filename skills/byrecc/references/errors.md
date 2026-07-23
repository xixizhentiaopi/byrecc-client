# ByreCC error handling

## Contents

- [Authentication and permission](#authentication-and-permission)
- [Billing and limits](#billing-and-limits)
- [Arguments and pagination](#arguments-and-pagination)
- [Provider and service](#provider-and-service)

## Authentication and permission

| Code | Meaning | Action |
|---|---|---|
| `AUTHENTICATION_FAILED` | Missing, invalid, revoked, or expired API key | Ask the user to run `byrectl login` or reinstall; never request the key in chat |
| `PERMISSION_DENIED` | Key cannot read the requested platform | Explain which platform was denied; ask the user to update the Key in the console |

Do not retry either error automatically.

## Billing and limits

| Code | Meaning | Action |
|---|---|---|
| `INSUFFICIENT_CREDITS` | Balance cannot cover the call | Report available/required values when provided and stop |
| `API_KEY_RATE_LIMITED` | Client-side request rate exceeded | Wait for `retry_after` when provided, then retry at most once |
| `IDEMPOTENCY_CONFLICT` | A request identity is already bound or running | Do not alter arguments and retry blindly; report the conflict |

Never recharge, upgrade a plan, or create a higher-budget Key without explicit user action.

## Arguments and pagination

| Code | Meaning | Action |
|---|---|---|
| `INVALID_ARGUMENTS` | One or more parameters are invalid | Correct only from the documented schema and retry once |
| `INVALID_CONTINUATION` | Token is invalid, expired, or belongs to another query | Discard it; restart from page one only if still needed |
| `CONTINUATION_UNAVAILABLE` | The operation does not support another page | Stop pagination and explain the provider limitation |

Never fabricate IDs or continuation values.

## Provider and service

| Code | Meaning | Retry policy |
|---|---|---|
| `PROVIDER_TIMEOUT` | Upstream timed out | Retry once after a short delay |
| `PROVIDER_REQUEST_FAILED` | Upstream request failed | Retry once after a short delay |
| `PROVIDER_RESPONSE_TOO_LARGE` | Provider response exceeded the safety limit | Do not retry unchanged; reduce the request if possible |
| `CAPABILITY_UNAVAILABLE` | Capability is disabled or has no active provider | Do not retry immediately |
| `RATE_LIMITER_UNAVAILABLE` | Admission service is unavailable | Retry once later |

If the second attempt fails, stop and provide the error code plus `trace_id` when available.

