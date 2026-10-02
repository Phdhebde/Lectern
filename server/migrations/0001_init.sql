-- Lectern initial schema.
-- Conventions: UUID primary keys, timestamptz everywhere, soft states expressed as enums in CHECK constraints.

-- ---------------------------------------------------------------------------
-- Organizations, users, memberships, roles
-- ---------------------------------------------------------------------------

CREATE TABLE organizations (
    id           UUID PRIMARY KEY,
    name         TEXT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('partner', 'customer')),
    -- Requirement level (e.g. partner tier) the organization currently holds or targets.
    level_slug   TEXT,
    -- Code shared by the training manager so learners can request to join.
    join_code    TEXT NOT NULL UNIQUE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id              UUID PRIMARY KEY,
    email           TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    -- Whether the public verification pages of this user's certificates are visible.
    public_profile  BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_login_at   TIMESTAMPTZ
);
CREATE UNIQUE INDEX users_email_key ON users (lower(email));

CREATE TABLE user_identities (
    issuer      TEXT NOT NULL,
    subject     TEXT NOT NULL,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (issuer, subject)
);

-- Platform-wide roles. Learner is implicit; training manager is per-organization (memberships.org_role).
CREATE TABLE user_roles (
    user_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role     TEXT NOT NULL CHECK (role IN ('channel_manager', 'trainer', 'admin')),
    PRIMARY KEY (user_id, role)
);

-- A user belongs to at most one organization. No membership = individual learner.
CREATE TABLE memberships (
    user_id      UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    org_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    status       TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'rejected')),
    org_role     TEXT NOT NULL DEFAULT 'learner' CHECK (org_role IN ('learner', 'training_manager')),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    decided_at   TIMESTAMPTZ,
    decided_by   UUID REFERENCES users(id) ON DELETE SET NULL
);
CREATE INDEX memberships_org_idx ON memberships (org_id, status);

-- ---------------------------------------------------------------------------
-- Authentication state
-- ---------------------------------------------------------------------------

CREATE TABLE sessions (
    -- SHA-256 of the session token held in the cookie; the raw token is never stored.
    token_hash   BYTEA PRIMARY KEY,
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    csrf_token   TEXT NOT NULL,
    -- True when the identity provider asserted a multi-factor authentication.
    mfa          BOOLEAN NOT NULL DEFAULT FALSE,
    method       TEXT NOT NULL CHECK (method IN ('oidc', 'email')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

CREATE TABLE login_tokens (
    token_hash  BYTEA PRIMARY KEY,
    email       TEXT NOT NULL,
    -- Name given at sign-up, applied when the account is created.
    display_name TEXT NOT NULL DEFAULT '',
    return_to   TEXT NOT NULL DEFAULT '/',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    used_at     TIMESTAMPTZ
);
CREATE INDEX login_tokens_email_idx ON login_tokens (lower(email), created_at);

-- Pending OIDC authorization requests (state -> PKCE verifier + nonce).
CREATE TABLE oidc_flows (
    state          TEXT PRIMARY KEY,
    pkce_verifier  TEXT NOT NULL,
    nonce          TEXT NOT NULL,
    return_to      TEXT NOT NULL DEFAULT '/',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Bearer tokens for machine integrations (e.g. partner portal export).
CREATE TABLE api_tokens (
    id           UUID PRIMARY KEY,
    name         TEXT NOT NULL,
    token_hash   BYTEA NOT NULL UNIQUE,
    created_by   UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    revoked_at   TIMESTAMPTZ
);

-- ---------------------------------------------------------------------------
-- Content: tracks, modules, scenarios, questions, assets
-- ---------------------------------------------------------------------------

CREATE TABLE assets (
    id            UUID PRIMARY KEY,
    sha256        TEXT NOT NULL UNIQUE,
    content_type  TEXT NOT NULL,
    size_bytes    BIGINT NOT NULL,
    original_name TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE tracks (
    id                  UUID PRIMARY KEY,
    slug                TEXT NOT NULL UNIQUE,
    title               TEXT NOT NULL,
    summary             TEXT NOT NULL DEFAULT '',
    description_md      TEXT NOT NULL DEFAULT '',
    -- Who may see and follow the track: any of 'public', 'partner', 'customer'.
    audiences           TEXT[] NOT NULL,
    position            INT NOT NULL DEFAULT 0,
    prerequisite_slug   TEXT,
    prerequisites_md    TEXT NOT NULL DEFAULT '',
    estimated_minutes   INT NOT NULL DEFAULT 0,
    -- Scenarios must be completed before the exam unlocks.
    scenarios_required  BOOLEAN NOT NULL DEFAULT FALSE,
    -- Certification validity in months; NULL = never expires.
    validity_months     INT,
    module_quiz_pass_percent INT NOT NULL DEFAULT 70,
    -- Exam definition (sections, attempts policy). See server/src/domain/exam.rs.
    exam                JSONB NOT NULL,
    -- Optional short recertification exam definition.
    recert_exam         JSONB,
    badge               JSONB NOT NULL DEFAULT '{}'::jsonb,
    published           BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE modules (
    id               UUID PRIMARY KEY,
    track_id         UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    slug             TEXT NOT NULL,
    position         INT NOT NULL,
    title            TEXT NOT NULL,
    video_url        TEXT,
    captions_url     TEXT,
    duration_minutes INT NOT NULL DEFAULT 0,
    -- Recap sheet ("fiche récapitulative"), Markdown.
    body_md          TEXT NOT NULL DEFAULT '',
    -- Attached downloadable files: [{"asset_id": "...", "label": "..."}]
    attachments      JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Deep link to the matching documentation page.
    doc_url          TEXT,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (track_id, slug)
);

CREATE TABLE scenarios (
    id              UUID PRIMARY KEY,
    track_id        UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    slug            TEXT NOT NULL,
    position        INT NOT NULL DEFAULT 0,
    title           TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('implementation', 'diagnostic')),
    -- Exam-only scenarios are case studies never shown in the learning path.
    exam_only       BOOLEAN NOT NULL DEFAULT FALSE,
    context_md      TEXT NOT NULL DEFAULT '',
    pitfalls_md     TEXT NOT NULL DEFAULT '',
    -- Troubleshooting family it maps to (free text, e.g. "gateway-offline").
    family          TEXT,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (track_id, slug)
);

CREATE TABLE scenario_steps (
    scenario_id   UUID NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
    position      INT NOT NULL,
    action_md     TEXT NOT NULL,
    image_asset   UUID REFERENCES assets(id) ON DELETE SET NULL,
    image_alt     TEXT NOT NULL DEFAULT '',
    -- [{"type":"box"|"arrow"|"marker","x":..,"y":..,"w":..,"h":..,"x2":..,"y2":..,"label":".."}]
    -- Coordinates are percentages of the image size so they survive resizing.
    annotations   JSONB NOT NULL DEFAULT '[]'::jsonb,
    expected_md   TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (scenario_id, position)
);

CREATE TABLE questions (
    id              UUID PRIMARY KEY,
    -- Stable identifier from the content pack, used for idempotent imports.
    ref             TEXT NOT NULL UNIQUE,
    track_id        UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    -- quiz: end-of-module or end-of-scenario check (answers revealed after submission)
    -- exam: certification bank (answers never revealed)
    -- recert: recertification bank
    -- case: case-study question attached to an exam-only scenario
    pool            TEXT NOT NULL CHECK (pool IN ('quiz', 'exam', 'recert', 'case')),
    module_id       UUID REFERENCES modules(id) ON DELETE CASCADE,
    scenario_id     UUID REFERENCES scenarios(id) ON DELETE CASCADE,
    -- choice: one or more correct choices; written: free text reviewed by an evaluator.
    format          TEXT NOT NULL DEFAULT 'choice' CHECK (format IN ('choice', 'written')),
    prompt_md       TEXT NOT NULL,
    -- [{"id":"a","text":"...","correct":true}]
    choices         JSONB NOT NULL DEFAULT '[]'::jsonb,
    explanation_md  TEXT NOT NULL DEFAULT '',
    active          BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX questions_track_pool_idx ON questions (track_id, pool) WHERE active;

-- Anonymous per-question aggregates, kept when learners delete their account.
CREATE TABLE question_stats (
    question_id  UUID PRIMARY KEY REFERENCES questions(id) ON DELETE CASCADE,
    answered     BIGINT NOT NULL DEFAULT 0,
    correct      BIGINT NOT NULL DEFAULT 0
);

-- Generic certification requirements per organization kind (e.g. partner tiers).
CREATE TABLE requirement_levels (
    slug         TEXT PRIMARY KEY,
    org_kind     TEXT NOT NULL CHECK (org_kind IN ('partner', 'customer')),
    name         TEXT NOT NULL,
    rank         INT NOT NULL,
    -- {"track-slug": required_valid_certifications}
    requirements JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  JSONB NOT NULL
);

-- ---------------------------------------------------------------------------
-- Learning progress
-- ---------------------------------------------------------------------------

CREATE TABLE enrollments (
    user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id      UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    enrolled_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_module   UUID REFERENCES modules(id) ON DELETE SET NULL,
    completed_at  TIMESTAMPTZ,
    rating        INT CHECK (rating BETWEEN 1 AND 5),
    PRIMARY KEY (user_id, track_id)
);

CREATE TABLE module_progress (
    user_id          UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    module_id        UUID NOT NULL REFERENCES modules(id) ON DELETE CASCADE,
    video_position   INT NOT NULL DEFAULT 0,
    video_completed  BOOLEAN NOT NULL DEFAULT FALSE,
    quiz_best_score  INT,
    completed_at     TIMESTAMPTZ,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, module_id)
);

CREATE TABLE scenario_progress (
    user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    scenario_id   UUID NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
    current_step  INT NOT NULL DEFAULT 0,
    completed_at  TIMESTAMPTZ,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, scenario_id)
);

-- ---------------------------------------------------------------------------
-- Exams and certifications
-- ---------------------------------------------------------------------------

CREATE TABLE exam_attempts (
    id               UUID PRIMARY KEY,
    user_id          UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id         UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    purpose          TEXT NOT NULL CHECK (purpose IN ('certification', 'recertification')),
    status           TEXT NOT NULL CHECK (status IN ('in_progress', 'pending_review', 'passed', 'failed')),
    -- Frozen exam paper: sections, drawn questions, shuffled choice order.
    paper            JSONB NOT NULL,
    -- {"<question uuid>": ["a","c"] | "written answer"}
    answers          JSONB NOT NULL DEFAULT '{}'::jsonb,
    current_section  INT NOT NULL DEFAULT 0,
    section_deadline TIMESTAMPTZ NOT NULL,
    -- Per-section results [{"score": 80, "passed": true}]
    results          JSONB NOT NULL DEFAULT '[]'::jsonb,
    paid             BOOLEAN NOT NULL DEFAULT FALSE,
    started_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at      TIMESTAMPTZ,
    reviewer_id      UUID REFERENCES users(id) ON DELETE SET NULL,
    review           JSONB
);
-- Only one exam session at a time per account.
CREATE UNIQUE INDEX exam_attempts_one_active ON exam_attempts (user_id) WHERE status = 'in_progress';
CREATE INDEX exam_attempts_user_track_idx ON exam_attempts (user_id, track_id, started_at);

-- Extra attempts granted beyond the free allowance (manually today, by payment later).
CREATE TABLE attempt_credits (
    id          UUID PRIMARY KEY,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id    UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    source      TEXT NOT NULL CHECK (source IN ('grant', 'payment')),
    reference   TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    consumed_by UUID REFERENCES exam_attempts(id) ON DELETE SET NULL
);
CREATE INDEX attempt_credits_available_idx ON attempt_credits (user_id, track_id) WHERE consumed_by IS NULL;

CREATE TABLE certifications (
    id             UUID PRIMARY KEY,
    user_id        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id       UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    attempt_id     UUID REFERENCES exam_attempts(id) ON DELETE SET NULL,
    issued_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at     TIMESTAMPTZ,
    product_major  TEXT,
    superseded_by  UUID REFERENCES certifications(id) ON DELETE SET NULL,
    revoked_at     TIMESTAMPTZ,
    revoke_reason  TEXT,
    -- Certified during a transition period (e.g. QCM only); must take the full exam at recertification.
    provisional    BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX certifications_user_idx ON certifications (user_id, track_id);
CREATE INDEX certifications_expiry_idx ON certifications (expires_at) WHERE revoked_at IS NULL AND superseded_by IS NULL;

CREATE TABLE expiry_alerts (
    certification_id UUID NOT NULL REFERENCES certifications(id) ON DELETE CASCADE,
    days_before      INT NOT NULL,
    sent_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (certification_id, days_before)
);

-- ---------------------------------------------------------------------------
-- E-mail outbox and audit log
-- ---------------------------------------------------------------------------

CREATE TABLE email_outbox (
    id           UUID PRIMARY KEY,
    to_address   TEXT NOT NULL,
    subject      TEXT NOT NULL,
    html_body    TEXT NOT NULL,
    text_body    TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    send_after   TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at      TIMESTAMPTZ,
    attempts     INT NOT NULL DEFAULT 0,
    last_error   TEXT
);
CREATE INDEX email_outbox_pending_idx ON email_outbox (send_after) WHERE sent_at IS NULL;

CREATE TABLE audit_log (
    id          BIGSERIAL PRIMARY KEY,
    at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    actor_id    UUID REFERENCES users(id) ON DELETE SET NULL,
    action      TEXT NOT NULL,
    target      TEXT,
    details     JSONB NOT NULL DEFAULT '{}'::jsonb,
    ip          TEXT
);
CREATE INDEX audit_log_at_idx ON audit_log (at DESC);
