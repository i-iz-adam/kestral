export interface SlashCommand {
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

export function parseSlashCommand(input: string): SlashCommand {
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

export const SLASH_HELP = [
  "/auto — turn planning mode off and approve anything already waiting, so tool calls stop asking for approval",
  "/plan on|off — turn planning-mode approval on or off",
  "/subagents on|off — turn sub-agent delegation on or off",
  "/help — show this list",
].join("\n");
