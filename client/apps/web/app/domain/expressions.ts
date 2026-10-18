import type { TelegramExpression, MessageAttachment } from "#shared/types";
export function expressionParts(text: string, expressions: TelegramExpression[]): { text: string; attachment?: MessageAttachment }[] {
  const parts: { text: string; attachment?: MessageAttachment }[] = [];
  let end = 0;
  for (const expression of [...expressions].sort((a, b) => a.offset - b.offset)) {
    if (!Number.isSafeInteger(expression.offset) || !Number.isSafeInteger(expression.length) || expression.offset < end || expression.length <= 0 || expression.offset + expression.length > text.length) continue;
    parts.push({ text: text.slice(end, expression.offset) });
    end = expression.offset + expression.length;
    parts.push({ text: text.slice(expression.offset, end), attachment: expression.attachment });
  }
  parts.push({ text: text.slice(end) });
  return parts;
}
