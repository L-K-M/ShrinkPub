<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import {
    ArchiveFileIcon,
    Button,
    Dropdown,
    Notification,
    ProgressBar,
    TitleBar,
    TrashIcon,
    getSystem7WindowStyle
  } from '@lkmc/system7-ui';

  import type { QualityTier, ShrinkJob, SystemColors } from '$lib/types';
  import { TauriService } from '$lib/tauri';
  import { WindowManager } from '$lib/windowManager';
  import { notifications } from '$lib/util/notifications';
  import { formatBytes, formatSaving } from '$lib/util/format';

  const QUALITY_OPTIONS: { value: QualityTier; label: string }[] = [
    { value: 'veryhigh', label: 'Very High' },
    { value: 'high', label: 'High' },
    { value: 'medium', label: 'Medium' },
    { value: 'low', label: 'Low' },
    { value: 'verylow', label: 'Very Low' },
    { value: 'terrible', label: 'Terrible' },
    { value: 'atrocious', label: 'Atrocious' }
  ];
  const QUALITY_STORAGE_KEY = 'shrinkpub.quality';

  const appWindow = getCurrentWindow();
  const windowManager = new WindowManager();

  let jobs = $state<ShrinkJob[]>([]);
  let quality = $state<QualityTier>('medium');
  let windowFocused = $state(true);
  let isWindowShaded = $state(false);
  let isDropActive = $state(false);
  let systemColors = $state<SystemColors | null>(null);

  const windowStyle = $derived(systemColors ? getSystem7WindowStyle(systemColors) : '');
  const totalSaved = $derived(
    jobs
      .filter((job) => job.status === 'done' && job.outcome)
      .reduce((sum, job) => sum + Math.max(0, job.outcome!.input_bytes - job.outcome!.output_bytes), 0)
  );
  const doneCount = $derived(jobs.filter((job) => job.status === 'done').length);

  onMount(() => {
    loadPersistedQuality();
    void loadSystemColors();

    const unlistenFocus = appWindow.onFocusChanged(({ payload }) => {
      windowFocused = payload;
    });

    const unlistenProgress = TauriService.listenShrinkProgress((progress) => {
      const job = jobs.find((candidate) => candidate.id === progress.job_id);
      if (job && job.status === 'working') {
        job.progress = progress;
      }
    });

    const unlistenDrop = appWindow.onDragDropEvent((event) => {
      if (event.payload.type === 'enter' || event.payload.type === 'over') {
        isDropActive = true;
      } else if (event.payload.type === 'leave') {
        isDropActive = false;
      } else if (event.payload.type === 'drop') {
        isDropActive = false;
        void addPaths(event.payload.paths);
      }
    });

    return () => {
      unlistenFocus.then((fn) => fn());
      unlistenProgress.then((fn) => fn());
      unlistenDrop.then((fn) => fn());
    };
  });

  function loadPersistedQuality() {
    try {
      const stored = localStorage.getItem(QUALITY_STORAGE_KEY);
      if (stored && QUALITY_OPTIONS.some((option) => option.value === stored)) {
        quality = stored as QualityTier;
      }
    } catch {
      // localStorage unavailable: keep the default.
    }
  }

  function persistQuality(value: QualityTier) {
    try {
      localStorage.setItem(QUALITY_STORAGE_KEY, value);
    } catch {
      // Not fatal — the choice just won't survive a restart.
    }
  }

  async function loadSystemColors() {
    try {
      systemColors = await TauriService.getSystemColors();
    } catch {
      systemColors = null;
    }
  }

  async function chooseFiles() {
    const selected = await open({
      multiple: true,
      filters: [{ name: 'EPUB', extensions: ['epub'] }]
    });
    if (!selected) return;
    await addPaths(Array.isArray(selected) ? selected : [selected]);
  }

  async function addPaths(paths: string[]) {
    if (paths.length === 0) return;

    let infos;
    try {
      infos = await TauriService.inspectPaths(paths);
    } catch (error) {
      notifications.add(error instanceof Error ? error.message : String(error), 'error');
      return;
    }

    for (const info of infos) {
      // Skip files that are already in the list and not finished.
      const active = jobs.some(
        (job) => job.path === info.path && job.status === 'working'
      );
      if (active) continue;

      const job: ShrinkJob = {
        id: crypto.randomUUID(),
        path: info.path,
        name: info.name,
        sizeBytes: info.size_bytes,
        status: 'working',
        progress: null,
        outcome: null,
        error: null
      };

      if (info.is_dir) {
        job.status = 'error';
        job.error = 'Folders are not supported — drop .epub files.';
      } else if (!info.is_epub) {
        job.status = 'error';
        job.error = "I'm no EPUB yet!";
      }

      jobs.push(job);
      if (job.status === 'working') {
        void runJob(job.id);
      }
    }
  }

  async function runJob(jobId: string) {
    const job = jobs.find((candidate) => candidate.id === jobId);
    if (!job) return;
    try {
      const outcome = await TauriService.shrinkEpubFile(job.id, job.path, quality);
      job.outcome = outcome;
      job.status = 'done';
    } catch (error) {
      job.error = error instanceof Error ? error.message : String(error);
      job.status = 'error';
      notifications.add(`${job.name}: ${job.error}`, 'error');
    } finally {
      job.progress = null;
    }
  }

  function removeJob(jobId: string) {
    jobs = jobs.filter((job) => job.id !== jobId);
  }

  function clearFinished() {
    jobs = jobs.filter((job) => job.status === 'working');
  }

  async function revealOutput(job: ShrinkJob) {
    if (!job.outcome) return;
    try {
      await revealItemInDir(job.outcome.output_path);
    } catch (error) {
      notifications.add(error instanceof Error ? error.message : String(error), 'error');
    }
  }

  function jobDetail(job: ShrinkJob): string {
    if (job.status === 'done' && job.outcome) {
      const saving = formatSaving(job.outcome.input_bytes, job.outcome.output_bytes);
      const sizes = `${formatBytes(job.outcome.input_bytes)} → ${formatBytes(job.outcome.output_bytes)}`;
      const images = `${job.outcome.images_recompressed} images recompressed`;
      return saving ? `${sizes} · saved ${saving} · ${images}` : `${sizes} · nothing saved · ${images}`;
    }
    if (job.status === 'error' && job.error) {
      return job.error;
    }
    return formatBytes(job.sizeBytes);
  }

  function handleWindowClose() {
    void windowManager.close();
  }

  async function handleWindowShade() {
    isWindowShaded = await windowManager.toggleShade();
  }

  function handleWindowDrag() {
    void windowManager.startDragging();
  }

  function handleQualityChange(value: string) {
    persistQuality(value as QualityTier);
  }
</script>

<div
  class="window-frame s7-root"
  class:window-unfocused={!windowFocused}
  class:drop-active={isDropActive}
  style={windowStyle}
>
  <TitleBar
    title="ShrinkPub"
    focused={windowFocused}
    closable
    shadeable
    draggable
    onclose={handleWindowClose}
    onshade={handleWindowShade}
    ondragstart={handleWindowDrag}
  />

  {#if !isWindowShaded}
    <main class="content">
      {#if jobs.length === 0}
        <div class="empty-state">
          <div class="arrow" aria-hidden="true">⇩</div>
          <p class="headline">Bring out your EPUBs!</p>
          <p class="subline">Drop .epub files anywhere in this window, or</p>
          <Button onclick={chooseFiles}>Choose Files…</Button>
        </div>
      {:else}
        <ul class="file-list">
          {#each jobs as job (job.id)}
            <li class="file-row" class:failed={job.status === 'error'}>
              <span class="file-icon"><ArchiveFileIcon size={28} alt="" /></span>
              <span class="texts">
                <span class="file-name" title={job.path}>{job.name}</span>
                {#if job.status === 'working'}
                  <span class="working">
                    <ProgressBar
                      value={job.progress ? job.progress.index + 1 : 0}
                      max={job.progress ? job.progress.total : 1}
                      height={10}
                      indeterminate={!job.progress}
                      ariaLabel={`Shrinking ${job.name}`}
                    />
                    <span class="meta">{job.progress ? job.progress.entry_name : 'Starting…'}</span>
                  </span>
                {:else}
                  <span class="meta" class:error-text={job.status === 'error'}>{jobDetail(job)}</span>
                {/if}
              </span>
              <span class="row-actions">
                {#if job.status === 'done'}
                  <Button onclick={() => revealOutput(job)}>Show</Button>
                {/if}
                <Button variant="icon" onclick={() => removeJob(job.id)} title="Remove from list">
                  <TrashIcon size={16} alt="Remove" />
                </Button>
              </span>
            </li>
          {/each}
        </ul>
        <div class="list-actions">
          <Button onclick={chooseFiles}>Add Files…</Button>
          <Button onclick={clearFinished} disabled={jobs.every((job) => job.status === 'working')}>
            Clear Finished
          </Button>
        </div>
      {/if}
    </main>

    <footer class="settings-bar">
      <label class="quality-label" for="quality">Quality</label>
      <span class="quality-select">
        <Dropdown id="quality" options={QUALITY_OPTIONS} bind:value={quality} onchange={handleQualityChange} />
      </span>
      {#if doneCount > 0 && totalSaved > 0}
        <span class="summary">Saved {formatBytes(totalSaved)} across {doneCount} {doneCount === 1 ? 'book' : 'books'}</span>
      {/if}
    </footer>
  {/if}

  <Notification notifications={$notifications} ondismiss={(id) => notifications.remove(id)} />
</div>

<style>
  .window-frame {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
    background: var(--system7-color-paper, #fff);
  }

  .drop-active .content {
    outline: 3px dashed var(--system7-color-ink, #000);
    outline-offset: -8px;
    background: var(--system7-color-highlight, #dcdcdc);
  }

  .content {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    text-align: center;
    padding: 24px;
  }

  .empty-state .arrow {
    font-size: 64px;
    line-height: 1;
  }

  .empty-state .headline {
    margin: 0;
    font-weight: bold;
  }

  .empty-state .subline {
    margin: 0 0 8px;
    font-size: 18px;
    opacity: 0.75;
  }

  .file-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .file-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid color-mix(in srgb, var(--system7-color-ink, #000) 15%, transparent);
  }

  .file-icon {
    flex: none;
    line-height: 0;
  }

  .texts {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .file-name {
    font-weight: bold;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .working {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .meta {
    font-size: 18px;
    opacity: 0.75;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta.error-text {
    color: var(--system7-color-error, #b00020);
    opacity: 1;
  }

  .row-actions {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .list-actions {
    display: flex;
    gap: 10px;
    padding: 10px 12px;
  }

  .settings-bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-top: 2px solid var(--system7-color-ink, #000);
    background: var(--system7-color-paper, #fff);
  }

  .quality-label {
    font-weight: bold;
  }

  .quality-select {
    min-width: 160px;
  }

  .summary {
    margin-left: auto;
    font-size: 18px;
    opacity: 0.75;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
