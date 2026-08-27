/** The L1 launch tutorial arc: ten purpose-built tutorial toys whose heavily
 *  commented Lua IS the tutorial (see cliban milestone "L1 · Public launch").
 *  One module per toy; order here = pedagogical order = seed order. */
import type { Demo } from "../kit";
import { firstLight } from "./firstLight";
import { parallaxSkyline } from "./parallaxSkyline";
import { mode7Road } from "./mode7Road";
import { spriteParade } from "./spriteParade";
import { cavernCamera } from "./cavernCamera";
import { stageLights } from "./stageLights";
import { splitScreen } from "./splitScreen";
import { transitions } from "./transitions";
import { extbgDirectColor } from "./extbgDirectColor";
import { spriteLimits } from "./spriteLimits";

export const TUTORIALS: Demo[] = [
  firstLight,
  parallaxSkyline,
  mode7Road,
  spriteParade,
  cavernCamera,
  stageLights,
  splitScreen,
  transitions,
  extbgDirectColor,
  spriteLimits,
];
