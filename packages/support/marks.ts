// A line of the shared policy with each `{name}` it holds filled in, every one at once so a value that
// itself holds a mark is left as it is; a mark nothing names stays.
export function fillMarks(template: string, values: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (mark, name: string) => values[name] ?? mark);
}
