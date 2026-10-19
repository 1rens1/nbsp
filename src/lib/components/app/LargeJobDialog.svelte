<script lang="ts">
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import { output } from '$lib/state/output.svelte';
	import { settings } from '$lib/state/settings.svelte';

	const total = $derived(output.sheets * settings.s.copies);
</script>

<AlertDialog.Root open={output.confirming} onOpenChange={(o) => !o && (output.confirming = false)}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>Print {total} sheets?</AlertDialog.Title>
			<AlertDialog.Description>
				{output.sheets} sheet{output.sheets === 1 ? '' : 's'} × {settings.s.copies} cop{settings.s.copies === 1 ? 'y' : 'ies'}.
			</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
			<AlertDialog.Action
				onclick={() => {
					output.confirming = false;
					output.print(true);
				}}
			>
				Print
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
