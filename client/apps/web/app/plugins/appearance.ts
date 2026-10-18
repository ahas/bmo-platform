import { DEFAULT_PALETTE, paletteDeclarations, paletteOf } from "bmo-ui/shared/palettes";
import { DEFAULT_THEME, themeDeclarations, themeOf } from "bmo-ui/shared/themes";

import { readPalette, usePalette } from "~/domain/palette";
import { readTheme, useTheme } from "~/domain/theme";

// The page in the signed-in account's theme and palette from its first frame (docs/web.md, 「Both on
// the page」): the server reads both for the page it sends and states their colours in the page's
// head, over the library's own, and the browser reads them again whenever somebody signs in or out.
export default defineNuxtPlugin(async () => {
  const theme = useTheme();
  const palette = usePalette();
  const { loggedIn } = useUserSession();

  if (import.meta.server && loggedIn.value) {
    const fetcher = useRequestFetch();

    [theme.value, palette.value] = await Promise.all([readTheme(fetcher), readPalette(fetcher)]);
  }

  if (import.meta.client) {
    watch(loggedIn, async (signedIn) => {
      [theme.value, palette.value] = signedIn ? await Promise.all([readTheme($fetch), readPalette($fetch)]) : [DEFAULT_THEME.id, DEFAULT_PALETTE.id];
    });
  }

  useHead({
    style: [
      {
        key: "bmo-appearance",
        tagPriority: "critical",
        innerHTML: computed(() => `:root { ${themeDeclarations(themeOf(theme.value))} ${paletteDeclarations(paletteOf(palette.value))} }`)
      }
    ]
  });
});
