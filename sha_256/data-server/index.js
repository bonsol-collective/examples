const express = require('express');
const app = express();
const PORT = 3000;

app.get('/', (req, res) => {
//   res.send('my-secret-password-2');
  const text = "my-secret-password-3";

  // Create buffer of exactly 32 bytes, filled with 0x00
  const buffer = Buffer.alloc(32, 0);

  // Write the string into the buffer
  buffer.write(text, 0, 'utf-8');

  // Set content type as plain text (or octet-stream if needed)
  res.set('Content-Type', 'text/plain');

  // Send exact 32 bytes
  res.send(buffer);
});

app.listen(PORT, () => {
  console.log(`Server running at http://localhost:${PORT}`);
});
