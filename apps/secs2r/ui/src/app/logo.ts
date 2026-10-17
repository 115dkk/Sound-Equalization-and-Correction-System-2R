// The two logos the rail and the window may show (docs/DEVIATIONS.md U12).
// "전문가 모드" is upstream's confirmed logo, read straight from the upstream
// tree so a new drawing there reaches the screen without a copy; "킹반인
// 모드" is SECS 2R's own (apps/secs2r/icons/logo). icons/render_icons.py
// makes the program and window icons from the same files.
import expert from "../../../../../src/secs/wizard/assets/logo_b8.svg";
import expertSmall from "../../../../../src/secs/wizard/assets/logo_b8_small.svg";
import civilian from "../../../icons/logo/civilian.svg";
import civilianSmall from "../../../icons/logo/civilian_small.svg";
import type { AppIcon } from "../ipc/bindings/AppIcon";

export const LOGOS: Record<AppIcon, string> = { expert, civilian };

/** The simplified drawings for 32 px and smaller. */
export const SMALL_LOGOS: Record<AppIcon, string> = { expert: expertSmall, civilian: civilianSmall };

export const DEFAULT_LOGO: AppIcon = "expert";
