// A text put on the clipboard. The clipboard API is only offered on a secure origin, so a copy that it
// refuses falls back to the selection the browser has always had.
export async function copyText(text: string): Promise<boolean> {
  if (navigator.clipboard !== undefined) {
    try {
      await navigator.clipboard.writeText(text);

      return true;
    } catch {
      // Falls through to the textarea below.
    }
  }

  const holder = document.createElement("textarea");

  holder.value = text;
  holder.setAttribute("readonly", "");
  holder.style.position = "fixed";
  holder.style.opacity = "0";
  document.body.appendChild(holder);
  holder.select();

  try {
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    document.body.removeChild(holder);
  }
}
