const currency = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
});

/** The single place integer cents become a display string. */
export function formatCents(cents: number): string {
  return currency.format(cents / 100);
}

export function formatDate(rfc3339: string): string {
  const date = new Date(rfc3339);
  if (Number.isNaN(date.getTime())) {
    return rfc3339;
  }
  return date.toLocaleString("en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  });
}
