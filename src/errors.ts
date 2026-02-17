export class TsqError extends Error {
  readonly code: string;
  readonly exitCode: number;
  readonly details?: unknown;

  constructor(code: string, message: string, exitCode = 1, details?: unknown) {
    super(message);
    this.name = "TsqError";
    this.code = code;
    this.exitCode = exitCode;
    this.details = details;
  }
}
