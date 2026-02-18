import { TsqError } from "../errors";

const STDIN_TIMEOUT_MS = 30_000;

export async function readStdinContent(): Promise<string> {
  const readAll = async (): Promise<string> => new Response(Bun.stdin.stream()).text();

  const timeout = new Promise<never>((_resolve, reject) => {
    const timer = setTimeout(() => {
      reject(
        new TsqError(
          "VALIDATION_ERROR",
          `stdin read timed out after ${STDIN_TIMEOUT_MS / 1000} seconds`,
          1,
        ),
      );
    }, STDIN_TIMEOUT_MS);
    if (typeof timer === "object" && "unref" in timer) {
      timer.unref();
    }
  });

  const result = await Promise.race([readAll(), timeout]);
  if (result.trim().length === 0) {
    throw new TsqError("VALIDATION_ERROR", "stdin content must not be empty", 1);
  }
  return result;
}
