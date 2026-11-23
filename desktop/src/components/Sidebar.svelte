<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { t } from "../lib/i18n";
  import { baseName } from "../lib/paths";
  import type { ProjectEntry, SessionMeta } from "../lib/protocol";
  import SessionRow from "./SessionRow.svelte";

  let {
    workspace,
    projects,
    sessions,
    sessionId,
    busyIds,
    onNewChat,
    onSelect,
    onRename,
    onArchive,
    onDelete,
    onProject,
  }: {
    workspace: string;
    projects: ProjectEntry[];
    sessions: SessionMeta[];
    sessionId: string;
    busyIds: ReadonlySet<string>;
    onNewChat: () => void;
    onSelect: (id: string) => void;
    onRename: (id: string, title: string) => void;
    onArchive: (id: string, archived: boolean) => void;
    onDelete: (id: string) => void;
    onProject: (path: string) => void;
  } = $props();

  let menuOpen = $state(false);
  let tab = $state<"active" | "archived">("active");
  let previousCurrentArchived = $state(false);
  let list = $state<HTMLDivElement | null>(null);

  let projectName = $derived(baseName(workspace) || workspace);
  let active = $derived(sessions.filter((item) => !item.archived));
  let archived = $derived(sessions.filter((item) => item.archived));
  let visible = $derived(tab === "archived" ? archived : active);

  // An archived conversation being open puts the rail on the archived tab;
  // unarchiving it returns to the active tab.
  $effect(() => {
    const currentArchived = archived.some((item) => item.id === sessionId);
    if (currentArchived) {
      tab = "archived";
    } else if (previousCurrentArchived && tab === "archived") {
      tab = "active";
    }
    previousCurrentArchived = currentArchived;
  });

  // A tab switch starts at the top of its own list instead of inheriting a
  // clamped scroll position from the other tab.
  $effect(() => {
    void tab;
    if (list) list.scrollTop = 0;
  });

  function chooseProject(path: string): void {
    menuOpen = false;
    if (path !== workspace) {
      tab = "active";
      onProject(path);
    }
  }

  function startChat(): void {
    tab = "active";
    onNewChat();
  }

  async function pickFolder(): Promise<void> {
    menuOpen = false;
    const picked = await openDialog({
      directory: true,
      multiple: false,
      defaultPath: workspace || undefined,
    });
    if (typeof picked === "string" && picked !== workspace) {
      tab = "active";
      onProject(picked);
    }
  }

  // Deleting is a single click: no confirmation card stands between the user
  // and the action they asked for.
  function requestDelete(item: SessionMeta): void {
    onDelete(item.id);
  }
</script>

<aside class="sidebar">
  <div class="sidebar-brand">Keelson</div>

  <div class="project">
    <button
      class="project-button"
      onclick={() => (menuOpen = !menuOpen)}
      title={workspace}
    >
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
      </svg>
      <span class="project-name">{projectName}</span>
      <svg
        class="chevron"
        width="12"
        height="12"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="m6 9 6 6 6-6" />
      </svg>
    </button>

    {#if menuOpen}
      <button
        class="menu-backdrop"
        aria-label={$t("app.dismiss")}
        onclick={() => (menuOpen = false)}
      ></button>
      <div class="project-menu">
        <div class="menu-label">{$t("sidebar.projects")}</div>
        <div class="project-list">
          {#each projects as project (project.path)}
            <button
              class="project-item"
              class:active={project.path === workspace}
              onclick={() => chooseProject(project.path)}
              title={project.path}
            >
              <span class="project-item-name">{project.name}</span>
              <span class="project-item-path">{project.path}</span>
            </button>
          {/each}
        </div>
        <button class="project-add" onclick={pickFolder}>
          + {$t("sidebar.openFolder")}
        </button>
      </div>
    {/if}
  </div>

  <button class="new-chat" onclick={startChat}>
    <span class="plus" aria-hidden="true">+</span>
    {$t("sidebar.newChat")}
  </button>

  <div class="session-tabs" role="tablist">
    <button
      class="session-tab"
      class:active={tab === "active"}
      role="tab"
      aria-selected={tab === "active"}
      onclick={() => (tab = "active")}
    >
      {$t("sidebar.conversations")}
    </button>
    <button
      class="session-tab"
      class:active={tab === "archived"}
      role="tab"
      aria-selected={tab === "archived"}
      onclick={() => (tab = "archived")}
    >
      {$t("sidebar.archived")}
      <span class="tab-count">{archived.length}</span>
    </button>
  </div>

  <div class="session-list" bind:this={list}>
    {#if visible.length === 0}
      <p class="session-empty">
        {tab === "archived"
          ? $t("sidebar.archivedEmpty")
          : $t("sidebar.empty")}
      </p>
    {/if}
    {#each visible as item (item.id)}
      <SessionRow
        {item}
        active={item.id === sessionId}
        archived={item.archived}
        working={busyIds.has(item.id)}
        {onSelect}
        {onRename}
        {onArchive}
        onRequestDelete={requestDelete}
      />
    {/each}
  </div>

</aside>
