import { For, Show } from "solid-js";
import { matchActions, type MobileAction } from "../lib/palette-actions";

/**
 * The "Actions" section of the switcher: rows from `matchActions`, under
 * a small heading like the desktop palette's section labels.
 */
export function PaletteActionRows(props: {
  query: string;
  onRun: (action: MobileAction) => void;
}) {
  const rows = () => matchActions(props.query);
  return (
    <Show when={rows().length > 0}>
      <p class="px-4 pt-3 pb-1 text-[12px] font-medium text-(--color-outl-fg-dim)">
        Actions
      </p>
      <For each={rows()}>
        {(row) => (
          <button
            type="button"
            data-action={row.action}
            class="flex w-full items-center gap-3 rounded-xl px-4 py-3 text-left text-[15px] active:bg-(--color-outl-accent)/10"
            onClick={() => props.onRun(row.action)}
          >
            <span aria-hidden="true" class="text-(--color-outl-accent)">›</span>
            <span class="flex-1 truncate">{row.label}</span>
          </button>
        )}
      </For>
    </Show>
  );
}
