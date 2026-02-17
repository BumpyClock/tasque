import { type Envelope, SCHEMA_VERSION } from "./types";

export const okEnvelope = <T>(command: string, data: T): Envelope<T> => ({
  schema_version: SCHEMA_VERSION,
  command,
  ok: true,
  data,
});

export const errEnvelope = (
  command: string,
  code: string,
  message: string,
  details?: unknown,
): Envelope<never> => ({
  schema_version: SCHEMA_VERSION,
  command,
  ok: false,
  error: {
    code,
    message,
    ...(details === undefined ? {} : { details }),
  },
});
