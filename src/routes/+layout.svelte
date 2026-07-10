<script lang="ts">
  import '@lkmc/system7-ui/styles.css';
  import UpdateNotice from '$lib/components/UpdateNotice.svelte';

  let { children } = $props();
</script>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: transparent;
  }
</style>

{@render children()}
<UpdateNotice />
