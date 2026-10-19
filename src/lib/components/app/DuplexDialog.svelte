<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import { Spinner } from '$lib/components/ui/spinner';
	import { output } from '$lib/state/output.svelte';

	const open = $derived(output.phase === 'fronts' || output.phase === 'flip');
	const n = $derived(output.flipSheets);
</script>

<!-- Escape is ignored: closing this without Cancel or Print backs would strand the job (the
     phase stays "flip" and Print stays disabled). Any other close path counts as Cancel. -->
<AlertDialog.Root {open} onOpenChange={(o) => !o && open && output.abort()}>
	<AlertDialog.Content escapeKeydownBehavior="ignore">
		<AlertDialog.Header>
			<AlertDialog.Title class="flex items-center gap-2">
				{#if output.phase === 'fronts'}<Spinner /> Printing fronts…{:else}Flip the stack{/if}
			</AlertDialog.Title>
			<AlertDialog.Description>
				{#if output.phase === 'fronts'}
					Wait for the printer to finish. The next step appears when it's done.
				{:else}
					When the printer has stopped, take the {n} printed sheet{n === 1 ? '' : 's'} from the output tray. Flip
					the whole stack over on the long edge of the paper and load it back into the paper feeder.
					Then continue.
				{/if}
			</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<Button variant="outline" onclick={() => output.abort()}>Cancel</Button>
			{#if output.phase === 'fronts'}
				<Button variant="secondary" onclick={() => (output.phase = 'flip')}>Skip waiting</Button>
			{:else}
				<Button onclick={() => output.printBacks()}>Print backs</Button>
			{/if}
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
