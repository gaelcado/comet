-- Every page in the page's card is one page (ds/page): Settings' pages and
-- a plugin's page start at the card's inner top, Settings' title 16 under
-- it (ds/layout TOP), its column 32 in from each side wide and 16 in a
-- narrow card (ds/layout `gutter`); the title is the same box on every
-- page (General, Appearance, Plugins' catalog), on the column's edge, which
-- is its cards' edge; and a conversation's words line up with its composer
-- down both edges at any width (screens/chat/dock). On the shell in
-- ui/shell_fixture.luau, the chat page in ui/chat_fixture.luau. (GitHub's
-- pull request board, a plugin's page, stands its title in the same box:
-- src/plugins/examples_tests.rs, where the plugin loads.)

local function box(line: string): { x: number, y: number, w: number, h: number }
	local x, y, w, h = line:match(" (%-?[%d%.]+),(%-?[%d%.]+) ([%d%.]+)x([%d%.]+)")
	return { x = tonumber(x) :: number, y = tonumber(y) :: number, w = tonumber(w) :: number, h = tonumber(h) :: number }
end
local function first(app: any, target: string): { x: number, y: number, w: number, h: number }
	return box(app:tree(target):match("[^\n]+") :: string)
end

test("pages start at their card's top, share its gutters and their title's box, and a chat's words line up with its composer", function()
	for _, width in { 1400, 860 } do
		local app = mount("shell_fixture.luau")
		app:resize(width, 800)
		app:frames(30)
		-- (Plugins' page listed: the app loads it beside the shell.)
		app:set_state("shell_fixture.plugins", true)
		app:command("settings_stub.sign_in")
		app:command("shell.settings")
		app:frames(40)
		local page = first(app, "#settings-page")
		local heading = first(app, '#settings-page >> role:heading "General"')
		local row = first(app, "#setting-language")
		expect(`{width}: top {heading.y - page.y}`).to_be(`{width}: top 16`)
		-- (The group's card stands a point outside its rows, by its edge.)
		local left, right = row.x - 1 - page.x, page.x + page.w - (row.x + row.w + 1)
		expect(math.abs(left - right) <= 1).to_be(true)
		if page.w < 640 then
			expect(`{width}: gutter {left}`).to_be(`{width}: gutter 16`)
		else
			expect(left >= 32).to_be(true)
		end
		-- The title on its cards' edge, not in at their rows' words.
		expect(`{width}: title at {heading.x}`).to_be(`{width}: title at {row.x - 1}`)

		-- Every page's title in that same box (the headline's line high),
		-- on its own cards' edge: a page with a line under its title, and a
		-- catalog with a choice on its title's line and tabs under it.
		for _, other in { { "Appearance", "#setting-scheme", 1 }, { "Plugins", "#catalog-card", 0 } } do
			local name, card, edge = other[1], other[2], other[3]
			app:click(`#settings-nav >> role:tab "{name}"`)
			app:frames(40)
			local title = first(app, `#settings-page >> role:heading "{name}"`)
			expect(`{width} {name}: {title.x},{title.y} {title.h}`).to_be(`{width} {name}: {heading.x},{heading.y} {heading.h}`)
			expect(`{width} {name}: card at {first(app, card).x - edge}`).to_be(`{width} {name}: card at {title.x}`)
		end

		-- A plugin's page: at the card's inner top, where Settings' page is.
		app:command("plugin_page")
		app:frames(40)
		local card = first(app, "#shell-page")
		local area = first(app, "#plugin-page-area")
		expect(`{width}: plugin page at {area.y - card.y}`).to_be(`{width}: plugin page at {page.y - card.y}`)
		expect(app:error()).to_be_nil()

		-- A conversation: its words' column on the composer's edges.
		local chat = mount("chat_fixture.luau")
		chat:resize(width - 300, 700)
		chat:frames(40)
		local reply = box(chat:tree("#transcript"):match('[^\n]*article "Reply"[^\n]*') :: string)
		local composer = first(chat, "#chat-composer")
		expect(`{width}: {reply.x} {reply.w}`).to_be(`{width}: {composer.x} {composer.w}`)
		expect(chat:error()).to_be_nil()
	end
end)
