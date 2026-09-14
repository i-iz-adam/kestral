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
