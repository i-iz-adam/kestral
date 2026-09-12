import type { SlashCommandDef } from "./slashCommands";

export default function SlashCommandMenu({
  commands,
  activeIndex,
  onSelect,
}: {
  commands: SlashCommandDef[];
  activeIndex: number;
  onSelect: (cmd: SlashCommandDef) => void;
}) {
  if (commands.length === 0) return null;
  return (
    <div className="slash-menu" role="listbox">
      {commands.map((cmd, i) => (
        <button
          type="button"
          key={cmd.name}
          role="option"
          aria-selected={i === activeIndex}
          className={"slash-menu-item" + (i === activeIndex ? " active" : "")}
          // onMouseDown (not onClick) fires before the textarea's blur, so
          // picking an item with the mouse doesn't first close the menu
          // out from under the click.
          onMouseDown={(e) => {
            e.preventDefault();
            onSelect(cmd);
          }}
        >
          <span className="slash-menu-usage">
            /{cmd.name}
            {cmd.args ? ` ${cmd.args}` : ""}
          </span>
          <span className="slash-menu-desc">{cmd.description}</span>
        </button>
      ))}
    </div>
  );
}
