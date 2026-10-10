// In-memory note store. Data is lost when the server restarts.
let nextId = 1;
const notes = new Map();

function list() {
  return [...notes.values()];
}

function create(title, body) {
  const note = { id: nextId++, title, body, createdAt: new Date().toISOString() };
  notes.set(note.id, note);
  return note;
}

function remove(id) {
  return notes.delete(id);
}

module.exports = { list, create, remove };
