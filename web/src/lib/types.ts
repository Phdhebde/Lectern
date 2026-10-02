export interface Instance {
  name: string;
  product_name: string;
  product_major_version: string | null;
  public_url: string;
  contact_email: string;
  legal_notice_url: string | null;
  privacy_policy_url: string | null;
  documentation_url: string | null;
  logo: string | null;
  favicon: string | null;
  locale: string;
  auth: { email: boolean; oidc: { label: string; url: string } | null };
}

export type Role = "channel_manager" | "trainer" | "admin";

export interface Membership {
  org_id: string;
  org_name: string;
  org_kind: "partner" | "customer";
  status: "pending" | "approved" | "rejected";
  org_role: "learner" | "training_manager";
}

export interface Me {
  id: string;
  email: string;
  display_name: string;
  public_profile: boolean;
  roles: Role[];
  mfa: boolean;
  mfa_required_for: string[];
  membership: Membership | null;
  csrf_token: string;
  pending_reviews: number;
}

export interface CatalogEntry {
  slug: string;
  title: string;
  summary: string;
  audiences: string[];
  estimated_minutes: number;
  prerequisite: string | null;
  certifying: boolean;
  module_count: number;
  enrolled: boolean;
  completed_modules: number;
  certification: { id: string; status: CertStatus; expires_at: string | null } | null;
}

export type CertStatus = "valid" | "expired" | "revoked" | "superseded";

export interface ModuleSummary {
  id: string;
  slug: string;
  title: string;
  position: number;
  duration_minutes: number;
  has_video: boolean;
  quiz_questions: number;
  content_completed: boolean;
  quiz_best_score: number | null;
  completed: boolean;
}

export interface ScenarioSummary {
  id: string;
  slug: string;
  title: string;
  kind: "implementation" | "diagnostic";
  family: string | null;
  steps: number;
  current_step: number;
  completed: boolean;
}

export interface SectionSummary {
  title: string;
  duration_minutes: number;
  pass_percent: number;
  items: string;
}

export interface ExamStatus {
  purpose: "certification" | "recertification" | null;
  sections: SectionSummary[];
  blockers: string[];
  eligibility: { Ok: string } | { Err: { reason: string; retry_at?: string } } | null;
  active_attempt: string | null;
  recert_opens_at: string | null;
  attempts_used: number;
  free_attempts: number | null;
  credits: number;
}

export interface TrackDetail {
  slug: string;
  title: string;
  summary: string;
  description_html: string;
  prerequisites_html: string;
  prerequisite: string | null;
  audiences: string[];
  estimated_minutes: number;
  validity_months: number | null;
  scenarios_required: boolean;
  quiz_pass_percent: number;
  exam: { sections: SectionSummary[]; free_attempts: number | null; cooldown_days: number };
  modules: ModuleSummary[];
  scenarios: ScenarioSummary[];
  exam_status: ExamStatus | null;
  certification: { id: string; status: CertStatus; issued_at: string; expires_at: string | null; provisional: boolean } | null;
  enrollment: { enrolled_at: string; last_module: string | null; completed_at: string | null; rating: number | null } | null;
}

export interface Choice {
  id: string;
  html: string;
  correct?: boolean;
}

export interface Question {
  id: string;
  prompt_html: string;
  format: "choice" | "written";
  choices: Choice[];
}

export interface QuizResult {
  score: number;
  passed: boolean;
  completed?: boolean;
  results: { question_id: string; correct: boolean; correct_choices: string[]; explanation_html: string }[];
}

export interface Annotation {
  type: "box" | "arrow" | "marker";
  x: number;
  y: number;
  w?: number;
  h?: number;
  x2?: number;
  y2?: number;
  label?: string;
}

export interface Step {
  position: number;
  action_html: string;
  image_url: string | null;
  image_alt: string;
  annotations: Annotation[];
  expected_html: string | null;
}

export type Answer = string[] | string;

export interface AttemptItem {
  question_id: string;
  prompt_html: string;
  format: "choice" | "written";
  choices: Choice[];
  answer: Answer | null;
  explanation_html?: string;
}

export interface AttemptView {
  id: string;
  track: { slug: string; title: string };
  purpose: string;
  status: "in_progress" | "pending_review" | "passed" | "failed";
  current_section: number;
  section_deadline: string;
  server_now: string;
  sections: { title: string; duration_minutes: number; pass_percent: number; items: number }[];
  visible_sections: {
    index: number;
    title: string;
    items: AttemptItem[];
    scenarios: { id: string; title: string; context_html: string; steps: Step[] }[];
  }[];
  results: { score: number | null; correct: number; total: number; needs_review: boolean; passed: boolean | null }[];
  started_at: string;
  finished_at: string | null;
  review: { comment?: string; decision?: string } | null;
  learner?: { name: string; email: string };
  review_full?: unknown;
}

export interface Certification {
  id: string;
  track_slug: string;
  track_title: string;
  issued_at: string;
  expires_at: string | null;
  status: CertStatus;
  provisional: boolean;
  verify_url: string;
  linkedin_add_url: string;
}

export interface LevelStatus {
  slug: string;
  name: string;
  rank: number;
  met: boolean;
  requirements: { track_slug: string; track_title: string; required: number; valid: number; missing: number }[];
}

export interface OrgOverview {
  id: string;
  name: string;
  kind: string;
  level: string | null;
  join_code: string | null;
  members: {
    id: string;
    name: string;
    email: string;
    status: string;
    org_role: string;
    requested_at: string;
    progress: { track_slug: string; track_title: string; modules_total: number; modules_completed: number }[] | null;
    certifications: { id: string; track_slug: string; track_title: string; issued_at: string; expires_at: string | null; provisional: boolean }[] | null;
  }[];
  valid_certifications: Record<string, number>;
  levels: LevelStatus[];
}
