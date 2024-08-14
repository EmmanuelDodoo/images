Sessions()

vim.g.builder = "cargo run"
vim.keymap.set("n", "<leader>t", function()
    Run("cargo check")
end)
