export function readQuoteImage(file: File, done: (value: string) => void) {
  const reader = new FileReader();
  reader.onload = () =>
    typeof reader.result === "string" && done(reader.result);
  reader.readAsDataURL(file);
}
