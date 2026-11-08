-- A project's files and the one they show, as one pane: Zeron's files
-- panel beside its editor (crates/ui/src/shell/files_panel.rs), here one
-- surface. The tree at the left, the file beside it, and one band across
-- the top of both: the tree's filter and tools in its first part, the
-- file's name and actions in the rest, one hairline under the whole of it,
-- the tree's divider starting under that; so the pane reads as one thing,
-- not two glued together.
--
-- The tree hides and shows: a toggle at the band's start (the sidebar's
-- glyph, its panel narrowing as the tree goes), ⇧⌘E while the pane has
-- focus (an app binds it everywhere with `ui.command` and `commands.toggle`),
-- or `shown` set from outside. The toggle never moves: it stands at the
-- pane's edge, over the tree's head while the tree shows and over the
-- file's while it doesn't, and the file's name slides to meet it. Hiding,
-- the tree stands where it is and the file's edge sweeps over it, taking
-- the room, its ink fading a little as it goes (ds/motion `content`, as
-- the sidebar folds); showing is the same back. `shown` is the caller's,
-- so it can be remembered. `part`: "tree" draws the tree alone, filling the
-- pane (an app's column of its own for it), "file" the file alone, the
-- toggle still saying whether that column is out.
--
-- Narrow (too little room for a file beside the tree: under the tree's
-- width and a file's least, 288 + 240), the two never share the row. With
-- no file open the tree takes the whole pane, until a file is chosen. With
-- one open the file has the whole pane: opening a file closes the tree, the
-- toggle brings the tree over the whole pane, and a file chosen there goes
-- back to the file. Whether the tree stands beside the file (`shown`) is
-- left as it was, so widening brings it back if it was out.
--
-- `FilesPane { shown = state, tree = { FileTree's props }, viewer = { FileViewer's props }, head = "line" }`.

local FileTree = require("@self/file_tree")
local FileViewer = require("@self/file_viewer")
local SidebarIcon = require("@self/sidebar_icon")
local IconButton = require("@self/icon_button").IconButton
local motion = require("@self/motion")
local locale = require("@self/locale")

export type Props = {
	-- Whether the tree shows beside the file (the caller's, remembered).
	shown: State<boolean>,
	tree: FileTree.Props,
	viewer: FileViewer.Props,
	-- The file's header shape (ds/file_viewer): one line by default.
	head: FileViewer.Head?,
	-- The tree's width beside the file (the theme's, or the app's).
	width: (number | State<number>)?,
	-- "tree" or "file": one part alone (else both, side by side); a function
	-- to follow (a file tab beside an app's tree column that comes and goes).
	part: ("tree" | "file" | () -> ("tree" | "file")?)?,
	-- The pane's last width, if the caller keeps it: a pane made again starts
	-- exactly as it was.
	room: State<number>?,
	-- Filled with `toggle`, for a command bound outside.
	commands: { [string]: () -> () }?,
	-- Narrow with a file open: whether the tree is out over it (the
	-- caller's, so a pane made again shows it as it was).
	over: State<boolean>?,
	[any]: any,
}

local OWN = { shown = true, tree = true, viewer = true, head = true, width = true, part = true, commands = true, room = true, over = true, children = true }

-- How the tree moves: the sidebar's slide, and its ink a beat quicker.
local SLIDE = { duration = 260, ease = "out(3)" }
local INK = { duration = 160 }
-- The keys that hide and show it.
local KEYS = "cmd+shift+e"
-- The least a file's side is worth having, with no file to show yet.
local FILE_LEAST = 240

local function num(token: string, fallback: number): number
	return (ui.theme.get(token) :: number?) or fallback
end

local M = {}
M.keys = KEYS

local made = ui.component(function(props: Props): Node
	local shown = props.shown
	local theme_w = num("zeron.files.tree", 288)
	local width_state: State<number>? = if props.width ~= nil and type(props.width) ~= "number" then props.width :: any else nil
	-- The tree's width now (a state's, followed).
	local function tree_w(): number
		if width_state then
			return width_state:get()
		end
		return (props.width :: number?) or theme_w
	end
	local function part_now(): ("tree" | "file")?
		local p = props.part
		return if type(p) == "function" then p() else p :: any
	end
	local head_h = num(if props.head == "stack" then "zeron.files.stack" else "zeron.files.head", 40)
	local square = num("zeron.icon_button.lg", 28)
	local pad = num("space.2", 8)
	-- Room the heads keep at their start: the inset, the toggle, a step.
	local lead_w = pad + square + pad

	-- How wide the pane is. (Kept by the caller, a pane made again knows it on its
	-- first frame, so nothing opens, closes or fades that was at rest.)
	local room = props.room or ui.use_state(0)
	local settled = false
	local doc = props.viewer.doc :: any
	local function open_doc(): any
		return if type(doc) == "function" then doc() else doc:get()
	end
	-- No room for a file beside the tree: the two never share the row.
	local function narrow(): boolean
		if part_now() ~= nil then
			return false
		end
		local w = room:get()
		return w > 0 and w < tree_w() + FILE_LEAST
	end
	-- Narrow with a file open: the tree over it (else the file alone).
	local over = props.over or ui.use_state(false)
	-- Whether the toggle, narrow with a file open, says the tree over it
	-- rather than beside it.
	local function overlaying(): boolean
		return narrow() and open_doc() ~= nil
	end
	local function out(): boolean
		return if overlaying() then over:get() else shown:get()
	end
	-- Narrow, the tree out has it all.
	local function whole(): boolean
		return out() and narrow()
	end
	-- A file opened, or the pane widened: the tree off the file (narrow, the
	-- file shows; wide, the tree is where `shown` says).
	local was_open: string? = nil
	do
		local d = open_doc()
		was_open = if type(d) == "table" then d.path else nil
	end
	ui.effect(function()
		local d = open_doc()
		local path = if type(d) == "table" then d.path else nil
		if path ~= was_open then
			was_open = path
			if over:peek() then
				over:set(false)
			end
		end
	end)
	ui.effect(function()
		if not narrow() and over:peek() then
			over:set(false)
		end
	end)
	-- The tree's room beside the file, eased as it comes and goes by the tree's own
	-- slide.
	local function dock_target(): number
		local part = part_now()
		if part == "tree" then
			return room:get()
		elseif part == "file" then
			return 0
		end
		return if whole() then room:get() elseif out() then tree_w() else 0
	end
	local dock_w = ui.use_state(dock_target())
	local dock_easing: any = nil
	local was_whole = whole()
	ui.effect(function()
		local to = dock_target()
		local now_whole = whole()
		-- (All of it already, the pane resizing: with the pane, not after it.)
		local follows = now_whole and was_whole
		was_whole = now_whole
		if dock_easing then
			dock_easing:cancel()
			dock_easing = nil
		end
		-- (One part alone: the pane's own, at once.)
		if not settled or follows or part_now() ~= nil then
			dock_w:set(to)
		else
			dock_easing = anime.animate(dock_w, motion.params({ value = to, duration = SLIDE.duration, ease = SLIDE.ease }))
		end
	end)
	-- Its ink (and the toggle's glyph): on while it is out; the ink eased
	-- once the pane has been measured, set before. (In, at the fade's floor,
	-- as it's left once the file's edge has covered it: a pane made again
	-- shows it as it was.)
	local lit = ui.use_state(out())
	local ink = ui.use_state(if out() then 1 else motion.FADE_FLOOR)
	local fading: any = nil
	ui.effect(function()
		local on = out()
		lit:set(on)
		if fading then
			fading:cancel()
			fading = nil
		end
		-- (Only a little: the edge covering it does the rest.)
		local to = if on then 1 else motion.FADE_FLOOR
		if settled then
			fading = anime.animate(ink, motion.params({ value = to, duration = motion.FADE_MS, ease = motion.EASE }))
		else
			ink:set(if on then 1 else motion.FADE_FLOOR)
		end
	end)
	local function toggle()
		if overlaying() then
			over:set(not over:peek())
		else
			shown:set(not shown:peek())
		end
	end
	if props.commands then
		props.commands.toggle = toggle
	end

	-- The tree's width: its own, or all it's been given (it never slides narrower,
	-- only away).
	local function tree_now(): number
		return math.max(tree_w(), dock_w:get())
	end
	-- The tree's props, beside the file.
	local function tree_props(): FileTree.Props
		local tp: FileTree.Props = table.clone(props.tree)
		-- A file chosen over the narrow pane's file (that one again too):
		-- back to the file.
		local opened = props.tree.on_open
		tp.on_open = function(path: string)
			if over:peek() then
				over:set(false)
			end
			if opened then
				opened(path)
			end
		end
		tp.head = head_h
		tp.lead = lead_w
		-- The toggle's room, in the tree's own points as it slides (its
		-- right edge less where the tree's start is): the head is cut
		-- there, so it never shows under the toggle. At rest that's left
		-- of the head's field (`lead`), so nothing changes there.
		tp.head_cut = function(): number
			return pad + square
		end
		tp.w = tree_now :: any
		tp.h_full = true
		return tp
	end
	-- Beside the file: at its full width, slid under the pane's edge as far
	-- as it is in.
	local function beside(): Node
		return ui.column {
			id = "files-tree-beside",
			absolute = true,
			left = 0,
			top = 0,
			bottom = 0,
			w = tree_now,
			-- (Its ink goes as the user hides it.)
			opacity = function(): number
				return ink:get()
			end,
			-- (Gone, its ink is out and nothing in it can be touched.)
			pointer_events = function(): PointerEvents
				return if dock_w:get() > 0.5 then "auto" else "none"
			end,
			FileTree(tree_props()),
			-- Its edge, from under the band (the band's hairline runs across
			-- both heads).
			-- (None with no file's side to part it from.)
			ui.div {
				absolute = true,
				right = 0,
				top = head_h,
				bottom = 0,
				w = 1,
				bg = "$border.subtle",
				opacity = function(): number
					return if dock_w:get() >= room:get() - 0.5 then 0 else 1
				end,
			},
		}
	end
	-- (Standing at the pane's left, its room clipping it as the file's edge
	-- sweeps over it.)
	local tree = ui.div {
		id = "files-tree-room",
		absolute = true,
		left = 0,
		top = 0,
		bottom = 0,
		w = dock_w,
		overflow = "hidden",
		pointer_events = "none",
		-- (The file alone, the tree in a column of its own: none here, so
		-- the window holds one tree, not one more under each file.)
		ui.show(function(): boolean
			return part_now() ~= "file"
		end, beside()),
	}
	-- The file, beside the tree's room; as the tree goes, its header keeps
	-- room for the toggle instead, so its name comes to meet it.
	local viewer_props: FileViewer.Props = table.clone(props.viewer)
	viewer_props.head = props.head
	viewer_props.lead = function(): number
		return math.max(0, lead_w - dock_w:get())
	end
	viewer_props.grow = 1
	viewer_props.min_w = 0
	viewer_props.h_full = true
	-- (With no file, what its side says goes before it has no room to say it.)
	viewer_props.opacity = function(): number
		local open = if type(doc) == "function" then doc() else doc:get()
		return if open then 1 else math.clamp((room:get() - dock_w:get() - 100) / 100, 0, 1)
	end

	-- The toggle, at the band's start, over whichever head is under it.
	local toggle_button = ui.div {
		absolute = true,
		z = 3,
		left = pad,
		top = (head_h - square) / 2,
		IconButton {
			label = locale.t("ds.files_pane.toggle_tree"),
			tooltip = locale.s("ds.files_pane.toggle_tree_tip", { keys = ui.shortcut_text(KEYS) }),
			tooltip_side = "bottom",
			size = "lg",
			tone = "quiet",
			on_click = toggle,
			SidebarIcon { open = lit, size = 16 },
		},
	}

	local box: { [any]: any } = {}
	for k, v in props :: any do
		if not OWN[k] then
			box[k] = v
		end
	end
	box.relative = true
	box.overflow = "hidden"
	box.on_resize = function(size: { w: number, h: number })
		if room:peek() ~= size.w then
			room:set(size.w)
		end
		if not settled then
			ui.after(0, function()
				settled = true
			end)
		end
	end
	box.on_key = function(e): boolean?
		if e.cmd and e.shift and not e.alt and not e.ctrl and string.lower(e.key) == "e" then
			toggle()
			return true
		end
		return nil
	end
	-- The tree's room beside the file.
	box[#box + 1] = ui.div {
		shrink = 0,
		h_full = true,
		w = dock_w,
	}
	-- (The toggle and the tree first, as they stand left of the file: Tab
	-- reaches them before it. Drawn over it by `z`.)
	box[#box + 1] = toggle_button
	box[#box + 1] = tree
	-- (The tree alone: no file beside it.)
	if props.part ~= "tree" then
		box[#box + 1] = FileViewer(viewer_props)
	end
	return ui.row(box)
end)

-- Typed as what it is, a table called as a function: through `__call` the
-- checker would match a call's props exactly rather than as an argument.
type FilesPane = typeof(M) & ((props: Props) -> Node)
local FilesPane: FilesPane = setmetatable(M, {
	__call = function(_, props: Props): Node
		return made(props)
	end,
}) :: any -- (the checker doesn't see a `__call` table as a function)
return FilesPane
