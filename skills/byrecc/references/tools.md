# ByreCC tool reference

## Contents

- [Supported platforms](#supported-platforms)
- [Shared response](#shared-response)
- [RedNote tools](#rednote-tools)
- [Zhihu tools](#zhihu-tools)
- [Pagination](#pagination)

## Supported platforms

| Platform | Channel | Public content kinds | Scope |
|---|---|---|---|
| 小红书 / RedNote / Xiaohongshu | `rednote` | post, trend, reply | `content:read` or `rednote:read` |
| 知乎 / Zhihu | `zhihu` | article, answer, trend, reply | `content:read` or `zhihu:read` |

There are no public user-profile, personalized-feed, or write tools.

## Shared response

All paid tools return this envelope:

```json
{
  "trace_id": "req_xxx",
  "records": [],
  "pagination": {
    "continuation": null,
    "has_more": false
  },
  "charge": {
    "units": 2,
    "balance": 998
  }
}
```

`pagination` is absent for detail operations. Normalized records may contain:

```text
uid, channel, kind, heading, snippet, body_text, web_url,
creator, media, signals, published_at, updated_at, labels,
rank, heat
```

Fields that are unavailable are omitted. Do not interpret omission as zero or false.

## RedNote tools

### `rednote.search_notes`

Search public RedNote notes. Cost: 2 credits.

| Parameter | Required | Values/default |
|---|---:|---|
| `phrase` | yes | non-empty string, maximum 200 characters |
| `ranking` | no | `relevance` (default), `newest`, `most_liked`, `most_discussed`, `most_saved` |
| `freshness` | no | `anytime` (default), `today`, `week`, `half_year` |
| `media_format` | no | `any` (default), `image`, `video` |
| `discovery_scope` | no | `any` (default), `seen`, `unseen`, `subscribed` |
| `proximity` | no | `anywhere` (default), `same_city`, `nearby` |
| `continuation` | no | opaque token returned by the previous identical search |

RedNote pages are normalized to 20 records. Do not pass personalized or location-related filters unless the user explicitly requested them.

### `rednote.trending`

Read public RedNote search trends. Cost: 2 credits.

| Parameter | Required | Values/default |
|---|---:|---|
| `batch_size` | no | 20 by default; 1–50 |
| `continuation` | no | opaque token from the previous identical call |

### `rednote.note_detail`

Read one public RedNote note. Cost: 1 credit.

| Parameter | Required | Values |
|---|---:|---|
| `entry_id` | yes | returned note `uid`; 1–200 ASCII letters, digits, `_` or `-` |

### `rednote.note_comments`

Read public comments for one RedNote note. Cost: 2 credits.

| Parameter | Required | Values/default |
|---|---:|---|
| `entry_id` | yes | returned note `uid` |
| `batch_size` | no | 20 by default; 1–50 |
| `continuation` | no | currently unsupported by the provider |

The current provider returns one bounded comment page. If it reports `CONTINUATION_UNAVAILABLE`, do not retry with another token.

## Zhihu tools

### `zhihu.search`

Search public Zhihu content. Cost: 2 credits.

| Parameter | Required | Values/default |
|---|---:|---|
| `phrase` | yes | non-empty string, maximum 200 characters |
| `batch_size` | no | 20 by default; 1–50 |
| `continuation` | no | opaque token returned by the previous identical search |

Search results can contain articles and answers. Route follow-up detail calls using each record's `kind`.

### `zhihu.hot`

Read public Zhihu hot searches. Cost: 2 credits.

| Parameter | Required | Values/default |
|---|---:|---|
| `batch_size` | no | 20 by default; 1–50 |
| `continuation` | no | opaque token from the previous identical call |

### `zhihu.article`

Read one public Zhihu article. Cost: 1 credit.

| Parameter | Required | Values |
|---|---:|---|
| `entry_id` | yes | returned article `uid` |

### `zhihu.answer`

Read one public Zhihu answer. Cost: 1 credit.

| Parameter | Required | Values |
|---|---:|---|
| `entry_id` | yes | returned answer `uid` |

### `zhihu.comments`

Read public comments for a Zhihu article or answer. Cost: 2 credits.

| Parameter | Required | Values/default |
|---|---:|---|
| `entry_id` | yes | returned article or answer `uid` |
| `entry_kind` | yes | `article` or `answer` |
| `batch_size` | no | 20 by default; 1–50 |
| `continuation` | no | opaque token returned by the previous identical call |

## Pagination

Continuation tokens are signed, opaque, query-bound, and time-limited.

- Never parse or edit a token.
- Never reuse it with changed filters, phrase, page size, tool, platform, content ID, or content kind.
- Stop when `has_more` is false or when enough evidence has been collected.
- If `INVALID_CONTINUATION` occurs, restart from the first page only when the user still needs the data.

