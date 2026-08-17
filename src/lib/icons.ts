// Registers the bundled Lucide collection so every `<Icon icon="lucide:*" />`
// renders offline from local data instead of the Iconify API. The collection
// resolves in its own chunk, keeping it out of the app's main bundle.
import Icon, { addCollection } from "@iconify/svelte";

const { default: lucide } = await import("@iconify-json/lucide/icons.json");

addCollection(lucide as Parameters<typeof addCollection>[0]);

export default Icon;
