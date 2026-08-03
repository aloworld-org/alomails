// The active product surface. `@product` is a build alias resolved by Vite
// (and tsconfig) to one of the surfaces in this folder — `workplace` by
// default, `mail` when built with ALO_PRODUCT=mail. Consumers import the
// surface and the types from here; nothing else in the app knows which product
// it is.
export type {
  Capability,
  ComposeInsert,
  ProductConsole,
  ProductModule,
  ProductSurface,
} from "./types";
export { surface } from "@product";
