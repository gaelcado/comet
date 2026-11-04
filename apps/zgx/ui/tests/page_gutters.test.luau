-- Every page in the page's card starts at the card's top and keeps one
-- rhythm there: Settings' pages and a plugin's page start at the card's
-- inner top, Settings' first words 16 under it (ds/layout TOP), its column
-- 32 in from each side wide and 16 in a narrow card (ds/layout `gutter`),
-- and a conversation's words line up with its composer down both edges at
-- any width (screens/chat/dock). On the shell in ui/shell_fixture.luau, the
-- chat page in ui/chat_fixture.luau.

local function box(line: string): { x: number, y: number, w: number }
	local x, y, w = line:match(" (%-?[%d%.]+),(%-?[%d%.]+) ([%d%.]+)x[%d%.]+")
	return { x = tonumber(x) :: number, y = tonumber(y) :: number, w = tonumber(w) :: number }
end
local function first(app: any, target: string): { x: number, y: number, w: number }
	return box(app:tree(target):match("[^\n]+") :: string)
end

test("pages start at their card's top, share its gutters wide and narrow, and a chat's words line up with its composer", function()
	for _, width in { 1400, 860 } do
		local app = mount("shell_fixture.luau")
		app:resize(width, 800)
		app:frames(30)
		app:command("settings_stub.sign_in")
		app:command("shell.settings")
		app:frames(40)
		local page = first(app, "#settings-page")
		local heading = first(app, '#settings-page >> role:heading "General"')
		local row = first(app, "#setting-language")
		expect(`{width}: top {heading.y - page.y}`).to_be(`{width}: top 16`)
		-- (The group's card stands a point inside the column, by its edge.)
		local left, right = row.x - 1 - page.x, page.x + page.w - (row.x + row.w + 1)
		expect(math.abs(left - right) <= 1).to_be(true)
		if page.w < 640 then
			expect(`{width}: gutter {left}`).to_be(`{width}: gutter 16`)
		else
			expect(left >= 32).to_be(true)
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
