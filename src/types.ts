export interface PlanItem {
  content: string;
  status: string;
}

export interface GitFile {
  path: string;
  status: string;
  staged: boolean;
  additions: number;
  deletions: number;
}

export interface GitDiff {
  files: GitFile[];
  patch: string;
}

export interface Connection {
  id: string;
  type: string;
  name: string;
  created_at: number;
  updated_at: number;
  status: "connected" | "disconnected" | "error" | "untested";
  account_name?: string | null;
  config: Record<string, string>;
}

export interface SessionDefaults {
  planning_enabled: boolean;
  subagents_enabled: boolean;
  graceful_stop: boolean;
}

export interface ToolCall {
  id: string;
  type: string;
  function: { name: string; arguments: string };
}

export interface ChatMessage {
  role: "system" | "user" | "assistant" | "tool" | "skill-loaded";
  content?: string | null;
  images?: string[] | null;
  tool_calls?: ToolCall[] | null;
  tool_call_id?: string | null;
  name?: string | null;
}

export interface SessionUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  cost: number;
}

export interface Session {
  id: string;
  title: string;
  mode: "coding" | "general";
  planning_enabled: boolean;
  graceful_stop: boolean;
  workspace: string;
  subagents_enabled: boolean;
  messages: ChatMessage[];
  created_at: number;
  updated_at?: number;
  pinned?: boolean;
  archived?: boolean;
  usage?: SessionUsage;
}

export interface Workspace {
  id: string;
  name: string;
  path: string;
}

export interface ToolCallEventPayload {
  session_id: string;
  call_id: string;
  name: string;
  status: "start" | "awaiting-approval" | "done" | "error";
  args?: unknown;
  result?: string;
  parent_call_id?: string;
}

export interface MessageEventPayload {
  session_id: string;
  role: string;
  content: string;
  images?: string[] | null;
  /** Ties this final message to the start/delta events for the same
   * streamed turn. Absent for the user's own (never-streamed) message. */
  request_id?: string | null;
}

export interface MessageStartEventPayload {
  session_id: string;
  request_id: string;
  role: string;
}

export interface MessageDeltaEventPayload {
  session_id: string;
  request_id: string;
  delta: string;
}

export interface MessageCancelEventPayload {
  session_id: string;
  request_id: string;
}

export interface TurnEndEventPayload {
  session_id: string;
  error?: string | null;
  reason?: "normal" | "stopped" | null;
}

export type TimelineItem =
  | {
      kind: "message";
      key: string;
      requestId?: string;
      role: string;
      content: string;
      images?: string[];
      streaming: boolean;
    }
  | { kind: "tool"; key: string; callId: string };

export type HistoryItem =
  | { kind: "message"; key: string; role: string; content: string; images?: string[] }
  | { kind: "tool"; key: string; call: ToolCallEventPayload };

export interface OmniRouteConfigPayload {
  mode: "local" | "remote";
  remote_url: string | null;
  api_key: string | null;
  default_model?: string | null;
  default_image_model?: string | null;
}

/** One rendered image, as the backend hands it over: `data_url` for
 * painting right now, `path` for everything that outlives this turn
 * (saving, copying, repainting a reopened session). */
export interface ImageArtifact {
  path: string;
  name: string;
  mime: string;
  data_url: string;
  bytes: number;
  revised_prompt?: string | null;
}

/** Stages a generate_image call moves through, emitted on
 * `agent://image-progress`. The card animates against these rather than
 * a percentage — there's no honest progress number to report for a
 * single opaque provider call, so the stages carry the information and
 * the animation carries the sense of motion. */
export type ImageGenStage =
  | "resolving"
  | "dispatched"
  | "rendering"
  | "saving"
  | "done"
  | "error";

/** Generation renders from nothing; editing reworks pixels that already
 * exist. The card needs to know which from the first event, because an
 * edit shows the source image being worked on rather than an empty
 * frame. */
export type ImageGenMode = "generate" | "edit";

export interface ImageProgressEventPayload {
  session_id: string;
  call_id: string;
  stage: ImageGenStage;
  mode?: ImageGenMode | null;
  model?: string | null;
  prompt?: string | null;
  message?: string | null;
  /** The image being edited, as a data: URL — edit path only. */
  source_data_url?: string | null;
}

export interface ImageReadyEventPayload {
  session_id: string;
  call_id: string;
  mode?: ImageGenMode | null;
  source_data_url?: string | null;
  model: string;
  prompt: string;
  title: string;
  size: string;
  images: ImageArtifact[];
}

export interface ModelInfo {
  id: string;
  owned_by?: string | null;
  context_length?: number | null;
}

export interface ModelsCache {
  models: ModelInfo[];
  fetched_at: number;
}

export interface ModelTestResult {
  ok: boolean;
  latency_ms: number;
  message: string;
}

export interface Skill {
  id: string;
  name: string;
  description: string;
  source: "builtin" | "installed" | "learned" | "project";
  enabled: boolean;
  triggers: string[];
  overridden: boolean;
}

export interface SkillProposal {
  id: string;
  kind: "create" | "update";
  target_id: string | null;
  name: string;
  description: string;
  content: string;
  triggers: string[];
  rationale: string;
  previous_content: string | null;
  based_on_session: string | null;
  created_at: number;
}

export interface GithubIssue {
  number: number;
  title: string;
  body?: string | null;
  state: string;
  html_url: string;
  comments?: number;
  user?: { login: string };
  pull_request?: unknown;
}

export interface GithubComment {
  id: number;
  body: string;
  user?: { login: string };
  created_at: string;
}

export interface GithubPr {
  number: number;
  title: string;
  body?: string | null;
  state: string;
  html_url: string;
  merged?: boolean;
  mergeable?: boolean | null;
  user?: { login: string };
}
