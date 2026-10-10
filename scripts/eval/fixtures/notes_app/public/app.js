const list = document.getElementById("notes");
const form = document.getElementById("note-form");

async function refresh() {
  const notes = await (await fetch("/api/notes")).json();
  list.innerHTML = notes.map((n) => `<li><strong>${n.title}</strong><p>${n.body}</p></li>`).join("");
}

form.addEventListener("submit", async (e) => {
  e.preventDefault();
  await fetch("/api/notes", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ title: title.value, body: body.value }),
  });
  form.reset();
  refresh();
});

refresh();
