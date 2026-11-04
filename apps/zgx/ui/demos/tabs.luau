-- Tabs, underline and pill; and a strip of tabs that reorder by dragging.

local Tabs = require("@ds/tabs")
local TabStrip = require("@ds/tab_strip")
local motion = require("@ds/motion")
local kit = require("@demos/kit")

local section = ui.state("tb_section", "conversation")
local view = ui.state("tb_view", "open")
local settings = ui.state("tb_settings", "general")
local small = ui.state("tb_small", "unified")
local strip_tabs = ui.state("tb_strip", { "Inbox", "Needs review", "Drafts", "Shipped", "Archive" })
local strip_on = ui.state("tb_strip_on", "Needs review")

-- A strip as the session tabs and the panel's are: drag a tab along it, and
-- click one to watch the chosen wash glide (ds/tab_strip, ds/motion's TAB_*).
local Strip = ui.component(function(): Node
	local refs: { [string]: any } = {}
	local offset = ui.use_state(0)
	local content = ui.ref()
	local function face(id: string): Node
		return ui.row {
			grow = 1,
			h_full = true,
			px = "$space.3",
			items = "center",
			ui.text { id, text_size = "$font.size.sm", weight = "$font.weight.medium" },
		}
	end
	local strip = TabStrip.new {
		key = "demo_tab",
		items = function(): { string }
			return strip_tabs:get()
		end,
		width = function(id: string): number
			local r = refs[id]
			return if r then r.width or 0 else 0
		end,
		content = content,
		offset = offset,
		selected = function(): string?
			return strip_on:get()
		end,
		face = face,
		commit = function(id: string, index: number)
			local list = table.clone(strip_tabs:peek())
			table.remove(list, table.find(list, id) :: number)
			table.insert(list, index, id)
			strip_tabs:set(list)
		end,
		h = 28,
		rounded = "$radius.lg",
	}
	local function tab(id: string): Node
		local ref = ui.ref()
		refs[id] = ref
		local hovered = ui.use_state(false)
		local function chosen(): boolean
			return strip_on:get() == id
		end
		return ui.row {
			ref = ref,
			role = "tab",
			label = id,
			selected = chosen,
			h = 28,
			shrink = 0,
			rounded = "$radius.lg",
			cursor = "pointer",
			bg = function(): string
				return if hovered:get() and not chosen() then "$hover" else "#00000000"
			end,
			color = function(): string
				return if chosen() or hovered:get() then "$text.primary" else "$text.secondary"
			end,
			opacity = function(): number
				return strip.opacity(id)
			end,
			on_hover = function(inside: boolean)
				hovered:set(inside)
			end,
			on_click = function()
				strip_on:set(id)
			end,
			on_press = function(e: any)
				strip.press(e)
			end,
			on_drag = function(): any
				strip.begin(id)
				return { demo_tab = id }
			end,
			drag_preview = function(): Node
				return strip.preview(id)
			end,
			on_drag_end = function()
				strip.ended()
			end,
			face(id),
		}
	end
	return ui.row(strip.zone {
		w = 380,
		h = 36,
		items = "center",
		role = "tablist",
		label = "Mail",
		strip.Frame(ui.scroll_x {
			offset = offset,
			grow = 1,
			min_w = 0,
			h = 28,
			fade_x = { 24, 24 },
			fade_overflow = true,
			ui.row {
				ref = content,
				relative = true,
				gap = "$zeron.space.hair",
				items = "center",
				strip.Highlight(),
				ui.each(strip.order, tab, {
					key = function(id: string): string
						return id
					end,
					direction = "row",
					gap = "$zeron.space.hair",
					animate = motion.tab_slide(),
				}),
			},
		}),
	})
end)

return {
	id = "tabs",
	title = "Tabs",
	about = "The views of one thing, the indicator springing from tab to tab. Underline for a page's sections; pill for a quiet run of views in a header. A tab may carry an icon and a count. A strip of tabs (the session tabs', the panel's) reorders by dragging.",
	demo = function()
		return kit.page(
			kit.block(
				"UNDERLINE, A PULL REQUEST'S SECTIONS",
				ui.column {
					w = 560,
					Tabs {
						value = section,
						label = "Pull request",
						options = {
							{ value = "conversation", label = "Conversation", icon = "chat-round-line", count = 8 },
							{ value = "commits", label = "Commits", icon = "git-branch", count = 3 },
							{ value = "checks", label = "Checks", icon = "checklist", count = 12 },
							{ value = "files", label = "Files", icon = "document", count = 21 },
						},
					},
				}
			),
			kit.block(
				"UNDERLINE, PLAIN",
				ui.column {
					w = 420,
					Tabs {
						value = settings,
						options = {
							{ value = "general", label = "General" },
							{ value = "agents", label = "Agents" },
							{ value = "git", label = "Git" },
							{ value = "beta", label = "Beta", disabled = true },
						},
					},
				}
			),
			kit.block(
				"PILL",
				Tabs {
					value = view,
					look = "pill",
					label = "Pull requests",
					options = {
						{ value = "open", label = "Open", count = 14 },
						{ value = "review", label = "Needs review", count = 3 },
						{ value = "mine", label = "Mine" },
						{ value = "closed", label = "Closed" },
					},
				},
				Tabs {
					value = small,
					look = "pill",
					size = "sm",
					options = {
						{ value = "unified", label = "Unified" },
						{ value = "split", label = "Split" },
					},
				}
			),
			kit.block("A STRIP THAT REORDERS: DRAG A TAB ALONG IT", Strip {})
		)
	end,
}
