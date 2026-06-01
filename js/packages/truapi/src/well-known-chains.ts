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
    "0xbf0488dbe9daa1de1c08c5f743e26fdc2a4ecd74cf87dd1b4b1eeb99ae4ef19f",
} as const satisfies WellKnownChain;
