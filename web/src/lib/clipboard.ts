/// Puts text on the clipboard, and says whether it got there.
///
/// The asynchronous clipboard is the way, but a webview can refuse it - and
/// one that does throws rather than ignoring the call - so the old way of
/// selecting a hidden field and copying that is tried before giving up.
export async function copy(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    return copyBySelecting(text)
  }
}

function copyBySelecting(text: string): boolean {
  const field = document.createElement('textarea')
  field.value = text
  field.setAttribute('readonly', '')
  field.style.position = 'fixed'
  field.style.opacity = '0'
  document.body.append(field)
  field.select()
  try {
    return document.execCommand('copy')
  } catch {
    return false
  } finally {
    field.remove()
  }
}
