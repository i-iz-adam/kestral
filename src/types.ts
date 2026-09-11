export interface ToolCall {
  id: string;
  type: string;
  function: { name: string; arguments: string };
}

export interface ChatMessage {
  role: "system" | "user" | "assistant" | "tool";
  content?: string | null;
  tool_calls?: ToolCall[] | null;
  tool_call_id?: string | null;
  name?: string | null;
}

export interface Session {
  id: string;
  title: string;
  mode: "coding" | "general";
  planning_enabled: boolean;
  workspace: string;
  linked_repo?: string | null;
  subagents_enabled: boolean;
  messages: ChatMessage[];
  created_at: number;
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
}

export interface OmniRouteConfigPayload {
  mode: "local" | "remote";
  remote_url: string | null;
  api_key: string | null;
}

export interface Skill {
  id: string;
  name: string;
  description: string;
  source: "builtin" | "installed";
  enabled: boolean;
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
