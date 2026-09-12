export interface SlashCommandDef {
  name: string;
  /** Argument placeholder shown after the name, e.g. "on|off" — omitted
   * entirely for commands that take none (help, auto). */
  args?: string;
  description: string;
}

/** Single source of truth for both the "/help" text and the filtering
 * command menu (SlashCommandMenu) — add a command here and it shows up
 * in both automatically. */
export const SLASH_COMMANDS: SlashCommandDef[] = [
  {
    name: "auto",
    description: "Turn planning mode off and approve anything already waiting, so tool calls stop asking for approval",
  },
  {
    name: "plan",
    args: "on|off",
    description: "Turn planning-mode approval on or off",
  },
  {
    name: "subagents",
    args: "on|off",
    description: "Turn sub-agent delegation on or off",
  },
  {
    name: "help",
    description: "Show this list",
  },
];

export const SLASH_HELP = SLASH_COMMANDS.map(
  (c) => `/${c.name}${c.args ? ` ${c.args}` : ""} — ${c.description}`
).join("\n");

export interface ParsedSlashCommand {
  cmd: string;
  arg: string;
}

/** True for anything that should be intercepted before it ever reaches
 * send_message — a leading "/" followed by a letter, so a message that
 * just happens to start with a literal "/" character (a path, a date)
 * doesn't get swallowed as a command attempt. */
export function looksLikeSlashCommand(input: string): boolean {
  return /^\/[a-zA-Z]/.test(input.trim());
}

export function parseSlashCommand(input: string): ParsedSlashCommand {
  const trimmed = input.trim().slice(1); // drop the leading "/"
  const spaceIdx = trimmed.search(/\s/);
  if (spaceIdx === -1) {
    return { cmd: trimmed.toLowerCase(), arg: "" };
  }
  return {
    cmd: trimmed.slice(0, spaceIdx).toLowerCase(),
    arg: trimmed.slice(spaceIdx + 1).trim().toLowerCase(),
  };
}

/** Drives the live-filtering command menu: "/" alone (or with nothing
 * else typed yet) matches everything, "/pl" narrows to names starting
 * with "pl", and once an argument is being typed ("/plan o") it narrows
 * to just that one command shown as a reference rather than disappearing. */
export function filterSlashCommands(input: string): SlashCommandDef[] {
  const trimmed = input.trimStart();
  if (!trimmed.startsWith("/")) return [];
  const body = trimmed.slice(1);
  const spaceIdx = body.search(/\s/);
  const namePart = (spaceIdx === -1 ? body : body.slice(0, spaceIdx)).toLowerCase();

  if (spaceIdx !== -1) {
    const exact = SLASH_COMMANDS.find((c) => c.name === namePart);
    return exact ? [exact] : [];
  }
  return SLASH_COMMANDS.filter((c) => c.name.startsWith(namePart));
}
