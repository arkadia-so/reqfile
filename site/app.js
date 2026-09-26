const copyButton = document.querySelector("#copy-command");
const command = document.querySelector("#setup-command");
const status = document.querySelector("#copy-status");

copyButton.addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(command.textContent);
    status.textContent =
      "Copied. Paste it into your coding agent to get started.";
    copyButton.setAttribute("aria-label", "Setup prompt copied");
  } catch {
    const selection = window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(command);
    selection.removeAllRanges();
    selection.addRange(range);
    status.textContent =
      "Select and copy the prompt, then paste it into your coding agent.";
  }
});
