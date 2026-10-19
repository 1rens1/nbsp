import { api } from "$lib/api";

export type Capabilities = Awaited<ReturnType<typeof api.capabilities>>;

/** What this machine has installed: optional tools switch features on, required ones are reported. */
class Caps {
	value = $state<Capabilities>({ office: false, virtualPrinter: false, epsonUtility: false, installer: false, scan: true, missing: [] });

	async refresh() {
		this.value = await api.capabilities();
	}
}

export const caps = new Caps();
