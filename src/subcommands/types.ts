export type Subcommand = {
  name: string;
  description: string;
  run: (argv: string[]) => Promise<number>;
};
