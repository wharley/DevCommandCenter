/**
 * Tempo relativo curto para timestamps de atividade (locale pt por defeito).
 */
export function formatRelativeTimeFromNow(
  input: Date | string | number | null | undefined,
  locale = "pt",
): string | null {
  if (input == null) return null;
  const d = input instanceof Date ? input : new Date(input);
  if (Number.isNaN(d.getTime())) return null;

  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  let diffSec = Math.round((d.getTime() - Date.now()) / 1000);
  if (Math.abs(diffSec) < 60) return rtf.format(diffSec, "second");

  let diffMin = diffSec / 60;
  if (Math.abs(diffMin) < 60) return rtf.format(Math.round(diffMin), "minute");

  let diffHr = diffMin / 60;
  if (Math.abs(diffHr) < 24) return rtf.format(Math.round(diffHr), "hour");

  let diffDay = diffHr / 24;
  if (Math.abs(diffDay) < 7) return rtf.format(Math.round(diffDay), "day");

  let diffWeek = diffDay / 7;
  if (Math.abs(diffWeek) < 4.34524) return rtf.format(Math.round(diffWeek), "week");

  let diffMonth = diffDay / 30;
  if (Math.abs(diffMonth) < 12) return rtf.format(Math.round(diffMonth), "month");

  const diffYear = diffDay / 365;
  return rtf.format(Math.round(diffYear), "year");
}
