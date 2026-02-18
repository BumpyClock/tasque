import type { Command } from "commander";
import type { TasqueService } from "../app/service";
import type { GlobalOpts } from "./parsers";

export interface RuntimeDeps {
  service: TasqueService;
  findTasqueRoot: () => string | null;
}

export interface ActionRender<TValue, TJson> {
  jsonData: (value: TValue) => TJson;
  human: (value: TValue) => void;
}

export type RunAction = <TValue, TJson>(
  command: Command,
  action: (opts: GlobalOpts) => Promise<TValue>,
  render: ActionRender<TValue, TJson>,
) => Promise<void>;
