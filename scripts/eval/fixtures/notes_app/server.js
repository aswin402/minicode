const express = require("express");
const path = require("path");
const notesRouter = require("./src/routes/notes");

const app = express();
const PORT = 3000;

app.use(express.json());
app.use(express.static(path.join(__dirname, "public")));
app.use("/api/notes", notesRouter);

app.listen(PORT, () => {
  console.log(`Notes app listening on http://localhost:${PORT}`);
});
