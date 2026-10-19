/** Sets data-fade-top / data-fade-bottom while content is hidden above/below, for the .scroll-fade mask. */
export function scrollFade(node: HTMLElement) {
	const update = () => {
		const { scrollTop, scrollHeight, clientHeight } = node;
		node.toggleAttribute("data-fade-top", scrollTop > 1);
		node.toggleAttribute("data-fade-bottom", scrollTop + clientHeight < scrollHeight - 1);
	};
	const ro = new ResizeObserver(update);
	ro.observe(node);
	for (const child of node.children) ro.observe(child);
	node.addEventListener("scroll", update, { passive: true });
	update();
	return {
		destroy() {
			ro.disconnect();
			node.removeEventListener("scroll", update);
		},
	};
}
