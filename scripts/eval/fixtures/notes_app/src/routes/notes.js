const express = require("express");
const store = require("../store");

const router = express.Router();

router.get("/", (_req, res) => res.json(store.list()));

router.post("/", (req, res) => {
  const { title, body } = req.body || {};
  if (!title) return res.status(400).json({ error: "title is required" });
  res.status(201).json(store.create(title, body || ""));
});

router.delete("/:id", (req, res) => {
  const ok = store.remove(Number(req.params.id));
  res.status(ok ? 204 : 404).end();
});

module.exports = router;
