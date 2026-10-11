import { Show } from "solid-js";

import type { PageMeta } from "@outl/shared/api/types";
import { journalHeading } from "@outl/shared/journal";

/**
 * The words inside the page header's `<h1>`. A journal reads as a day
 * (`Today`, `Yesterday`, `Thursday`) with the full date beside it; its
 * ISO slug is the file name and lives in the eyebrow instead. A page
 * shows its title. Today takes the accent, the one place the header
 * uses it.
 */
export function PageTitle(props: { page: PageMeta }) {
  const heading = () =>
    props.page.kind === "journal" ? journalHeading(props.page.slug) : null;
  return (
    <Show when={heading()} fallback={props.page.title || props.page.slug}>
      {(h) => (
        <>
          <span
            class={
              h().title === "Today" ? "text-(--color-outl-accent)" : undefined
            }
          >
            {h().title}
          </span>
          <span class="ml-2.5 text-[15px] font-normal tracking-normal text-(--color-outl-fg-dim)">
            {h().date}
          </span>
        </>
      )}
    </Show>
  );
}
