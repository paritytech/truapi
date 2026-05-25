/** Well-known chain descriptors. Each chain is its own `export const` so that
 * bundlers can tree-shake the ones a consumer does not import. */

import type { HexString } from "./scale.js";

export interface WellKnownChain {
  readonly name: string;
  readonly network: "Mainnet" | "Testnet";
  readonly genesis: HexString;
}

export const PASEO_NEXT_V2_ASSET_HUB = {
  name: "Paseo Next v2 Hub",
  network: "Testnet",
  genesis:
    "0x173cea9df45656cf612c8b8ece56e04e9a693c69cfaac47d3628dae735067af8",
} as const satisfies WellKnownChain;
