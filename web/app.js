const form = document.querySelector("#speech");
const text = document.querySelector("#line");
const count = document.querySelector("#count");
const render = document.querySelector("#render");
const status = document.querySelector("#status");
const result = document.querySelector("#result");
const audio = document.querySelector("#audio");
const download = document.querySelector("#download");
let wavUrl;

function setStatus(message, error = false) {
  status.textContent = message;
  status.toggleAttribute("data-error", error);
}

function updateCount() {
  count.textContent = `${text.value.length} / ${text.maxLength}`;
}

text.addEventListener("input", updateCount);
updateCount();

const formats = {
  pitch: (value) => `${value} Hz`,
  speed: (value) => `${value.toFixed(2)}×`,
  formant_scale: (value) => `${value.toFixed(2)}×`,
  breath: (value) => `${(value * 100).toFixed(1)}%`,
};

for (const [id, format] of Object.entries(formats)) {
  const slider = form.elements.namedItem(id);
  const label = document.getElementById(`${id}-value`);
  const update = () => { label.value = format(slider.valueAsNumber); };
  slider.addEventListener("input", update);
  update();
}

form.addEventListener("change", (event) => {
  if (event.target.name !== "preset") return;
  const breath = form.elements.namedItem("breath");
  breath.value = event.target.value === "robot" ? "0" : breath.defaultValue;
  breath.dispatchEvent(new Event("input"));
});

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const fields = new FormData(form);
  const line = fields.get("text").trim();
  if (!line) {
    setStatus("Enter a line of dialogue.", true);
    text.focus();
    return;
  }
  const request = { text: line, preset: fields.get("preset") };
  for (const id of Object.keys(formats)) request[id] = Number(fields.get(id));

  render.disabled = true;
  setStatus("Rendering…");
  try {
    const response = await fetch("api/speak", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    });
    if (!response.ok) {
      setStatus(await response.text(), true);
      return;
    }
    const nextUrl = URL.createObjectURL(await response.blob());
    audio.src = nextUrl;
    download.href = nextUrl;
    if (wavUrl) URL.revokeObjectURL(wavUrl);
    wavUrl = nextUrl;
    result.hidden = false;
    setStatus("Ready to play.");
    try {
      await audio.play();
      setStatus("");
    } catch {
      // Some browsers require a separate click on the audio controls.
    }
  } catch {
    setStatus("Could not reach the jTalk server.", true);
  } finally {
    render.disabled = false;
  }
});
