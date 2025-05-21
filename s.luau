local fs = require("@lune/fs")
local process = require("@lune/process")

fs.writeFile(process.env.TMP.."/hi.txt", "hi.txt")