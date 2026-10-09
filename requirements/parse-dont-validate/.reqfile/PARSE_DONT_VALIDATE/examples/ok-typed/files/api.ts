import { z } from "zod";

const User = z.object({ name: z.string() });

export function parse(input: unknown): string {
  return User.parse(input).name;
}
