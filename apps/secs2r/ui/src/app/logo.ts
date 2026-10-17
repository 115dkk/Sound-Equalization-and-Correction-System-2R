// The two upstream logos the rail and the window may show (docs/DEVIATIONS.md
// U12). They come straight from the upstream tree, so a new drawing there
// reaches the screen without a copy; apps/secs2r/icons/render_icons.py makes
// the program icons from the same files.
import bubble from "../../../../../src/secs/wizard/assets/logo_b6.svg";
import bubbleSmall from "../../../../../src/secs/wizard/assets/logo_b6_small.svg";
import drip from "../../../../../src/secs/wizard/assets/logo_b8.svg";
import dripSmall from "../../../../../src/secs/wizard/assets/logo_b8_small.svg";
import type { AppIcon } from "../ipc/bindings/AppIcon";

export const LOGOS: Record<AppIcon, string> = { drip, bubble };

/** Upstream's simplified drawings for 32 px and smaller. */
export const SMALL_LOGOS: Record<AppIcon, string> = { drip: dripSmall, bubble: bubbleSmall };

export const DEFAULT_LOGO: AppIcon = "drip";
