/**
 * Cross-module type barrel. Each module owns its types and exports them
 * locally; this file re-exports them so callers have a single import site.
 *
 * Per spec §`src/types.ts`. `FetchedScript` is added in step 8.
 */
export type { InstallCommand, ParseError } from "./installer/parse.ts";
export type { Invocation, Outcome } from "./store/invocations.ts";
export type { PackageRow } from "./store/packages.ts";
