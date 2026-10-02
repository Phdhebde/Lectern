# HTTP API

The front-end uses the JSON API under `/api`. Authentication is the session cookie;
state-changing requests need the `X-CSRF-Token` header (value from `GET /api/me`). Errors
are `{"error": "<code>", "message": "…", "details": …}` with a 4xx/5xx status.

## Integration API (partner portal)

`GET /api/v1/certified` with `Authorization: Bearer <token>` (tokens: *Administration ›
API access*):

```json
{
  "generated_at": "2026-10-02T12:00:00Z",
  "organizations": [
    { "id": "…", "name": "Partner A", "kind": "partner",
      "valid_certifications": { "associate": 2, "engineer": 3 } }
  ]
}
```

Counts are distinct approved members holding a valid (not expired, revoked or superseded)
certification. A CSV with the same data: `GET /api/partners/export.csv` (channel managers,
administrators).

## Public endpoints

| Endpoint | Content |
| --- | --- |
| `GET /verify/{id}` | Verification page (HTML) |
| `GET /verify/{id}/badge.svg`, `badge.png` | Badge of a certification |
| `GET /ob/issuer` | Open Badges 2.0 issuer profile |
| `GET /ob/badges/{track}` | BadgeClass; image at `/ob/badges/{track}/image.png` |
| `GET /ob/assertions/{id}` | Assertion (hosted verification, hashed recipient) |
| `GET /api/instance`, `/theme.css` | Instance settings and theme |
| `GET /api/catalog` | Tracks visible to the caller |

## Deep links from the documentation

Link to `https://<academy>/tracks/<track>` or
`https://<academy>/tracks/<track>/modules/<module>`; modules link back to the documentation
with their `doc_url`.
