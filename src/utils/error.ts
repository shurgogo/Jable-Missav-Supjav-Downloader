export interface AppError {
  code: string;
  params?: Record<string, any>;
}

/**
 * Parses raw error strings or objects returned from Rust backend into structured AppError
 */
export function parseAppError(err: unknown): AppError {
  if (!err) {
    return { code: "UNKNOWN_ERROR" };
  }

  if (typeof err === "string") {
    const trimmed = err.trim();
    if (trimmed.startsWith("{") && trimmed.endsWith("}")) {
      try {
        const parsed = JSON.parse(trimmed);
        if (parsed && typeof parsed.code === "string") {
          return {
            code: parsed.code,
            params: parsed.params && typeof parsed.params === "object" ? parsed.params : undefined,
          };
        }
      } catch {
        // Fallback to text parsing below
      }
    }

    if (trimmed.includes("403 Forbidden") || trimmed.includes("CF_VERIFICATION_REQUIRED")) {
      return { code: "CF_VERIFICATION_REQUIRED" };
    }

    if (trimmed.includes("Not a directory") || trimmed.includes("DIRECTORY_CREATE_FAILED")) {
      return { code: "DIRECTORY_CREATE_FAILED", params: { reason: trimmed } };
    }

    return { code: "UNKNOWN_ERROR", params: { message: trimmed } };
  }

  if (typeof err === "object" && "code" in err && typeof (err as any).code === "string") {
    return {
      code: (err as any).code,
      params: (err as any).params,
    };
  }

  return { code: "UNKNOWN_ERROR", params: { message: String(err) } };
}

/**
 * Formats AppError into localized message using i18n dictionary and param interpolation
 */
export function formatErrorMessage(
  err: AppError,
  t: (key: string, params?: Record<string, any>) => string
): string {
  const i18nKey = `err_${err.code}`;
  const translated = t(i18nKey, err.params);

  if (translated && translated !== i18nKey) {
    return translated;
  }

  // Fallback formatting if key is missing
  if (err.params) {
    const details = Object.entries(err.params)
      .map(([k, v]) => `${k}: ${v}`)
      .join(", ");
    return `${err.code} (${details})`;
  }
  return err.code;
}
