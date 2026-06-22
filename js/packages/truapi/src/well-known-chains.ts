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

export const PASEO_NEXT_V2_INDIVIDUALITY = {
  name: "Paseo Next v2 Individuality",
  network: "Testnet",
  genesis:
    "0xc5af1826b31493f08b7e2a823842f98575b806a784126f28da9608c68665afa5",
} as const satisfies WellKnownChain;
