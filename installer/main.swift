import AppKit
import SwiftUI
import UniformTypeIdentifiers
import CoreText

// Copy beside the destination, then rename on the same filesystem. A failed copy leaves the installed app intact.
func installBundle(source: URL, destination: URL) throws {
    let fm = FileManager.default
    let info = NSDictionary(contentsOf: source.appendingPathComponent("Contents/Info.plist"))
    guard info?["CFBundleIdentifier"] as? String == "app.pulsedeck.desktop" else {
        throw NSError(domain: "PulseDeck", code: 1, userInfo: [NSLocalizedDescriptionKey: "The PulseDeck application is missing or invalid. Reopen the original download."])
    }
    if (try? destination.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true {
        throw NSError(domain: "PulseDeck", code: 2, userInfo: [NSLocalizedDescriptionKey: "The destination is a link. Choose a normal Applications folder."])
    }
    try fm.createDirectory(at: destination.deletingLastPathComponent(), withIntermediateDirectories: true)
    let stage = destination.deletingLastPathComponent().appendingPathComponent(".PulseDeck-install-\(UUID().uuidString).app")
    defer { try? fm.removeItem(at: stage) }
    try fm.copyItem(at: source, to: stage)
    if fm.fileExists(atPath: destination.path) {
        _ = try fm.replaceItemAt(destination, withItemAt: stage)
    } else {
        try fm.moveItem(at: stage, to: destination)
    }
}

let skillNames = ["pulsedeck-control", "pulsedeck-library", "djing-art"]
let legacySkillNames = ["track-inspection", "set-preparation", "prepare-next-track", "transition", "session-dj", "recording-and-export", "setup-and-troubleshooting"].map { "pulsedeck-" + $0 }

// Stage everything first and retain existing packages outside the skill search path.
func installSkills(source: URL, codexHome: URL) throws -> URL? {
    let fm = FileManager.default
    for name in skillNames {
        guard fm.fileExists(atPath: source.appendingPathComponent(name + "/SKILL.md").path) else {
            throw NSError(domain: "PulseDeck", code: 3, userInfo: [NSLocalizedDescriptionKey: "The bundled DJ skills are incomplete."])
        }
    }
    let destination = codexHome.appendingPathComponent("skills")
    if (try? destination.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true {
        throw NSError(domain: "PulseDeck", code: 4, userInfo: [NSLocalizedDescriptionKey: "The skills folder is a link. Install skills manually into its intended destination."])
    }
    try fm.createDirectory(at: destination, withIntermediateDirectories: true)
    let stage = codexHome.appendingPathComponent(".pulsedeck-skills-\(UUID().uuidString)")
    defer { try? fm.removeItem(at: stage) }
    try fm.createDirectory(at: stage, withIntermediateDirectories: true)
    for name in skillNames { try fm.copyItem(at: source.appendingPathComponent(name), to: stage.appendingPathComponent(name)) }
    let backup = codexHome.appendingPathComponent("pulsedeck-skill-backups/\(UUID().uuidString)")
    var saved = [String](), published = [String]()
    do {
        for name in skillNames + legacySkillNames {
            let item = destination.appendingPathComponent(name)
            if fm.fileExists(atPath: item.path) || (try? item.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true {
                try fm.createDirectory(at: backup, withIntermediateDirectories: true)
                try fm.moveItem(at: item, to: backup.appendingPathComponent(name))
                saved.append(name)
            }
        }
        for name in skillNames {
            try fm.moveItem(at: stage.appendingPathComponent(name), to: destination.appendingPathComponent(name))
            published.append(name)
        }
    } catch {
        for name in published { try? fm.removeItem(at: destination.appendingPathComponent(name)) }
        var rollbackErrors = [String]()
        for name in saved {
            do { try fm.moveItem(at: backup.appendingPathComponent(name), to: destination.appendingPathComponent(name)) }
            catch { rollbackErrors.append(name) }
        }
        if !rollbackErrors.isEmpty {
            throw NSError(domain: "PulseDeck", code: 5, userInfo: [NSLocalizedDescriptionKey: "Skill installation failed. Recover saved skills from \(backup.path): \(rollbackErrors.joined(separator: ", "))."])
        }
        throw error
    }
    return saved.isEmpty ? nil : backup
}

final class InstallModel: ObservableObject {
    @Published var dragging = false
    @Published var hovering = false
    @Published var copying = false
    @Published var installed = false
    @Published var error: String?
    @Published var pointer = CGPoint(x: 430, y: 260)
    @Published var confirmReplace = false
    @Published var includeSkills = true
    @Published var skillMessage: String?
    let codexHome = ProcessInfo.processInfo.environment["CODEX_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) } ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex")
    let source: URL
    let destination: URL
    init() {
        source = Bundle.main.bundleURL.deletingLastPathComponent().appendingPathComponent("PulseDeck.app")
        let fm = FileManager.default
        let folder = fm.isWritableFile(atPath: "/Applications") ? URL(fileURLWithPath: "/Applications") : fm.homeDirectoryForCurrentUser.appendingPathComponent("Applications")
        destination = folder.appendingPathComponent("PulseDeck.app")
    }
    func requestInstall() {
        guard !copying && !installed else { return }
        if NSWorkspace.shared.runningApplications.contains(where: { $0.bundleURL?.standardizedFileURL == destination.standardizedFileURL }) {
            error = "Quit the installed PulseDeck before replacing it. Your music and library will stay in place."
            return
        }
        if FileManager.default.fileExists(atPath: destination.path) { confirmReplace = true }
        else { install() }
    }
    func install() {
        guard !copying else { return }
        copying = true; dragging = false; hovering = false; error = nil
        let source = source, destination = destination, includeSkills = includeSkills, codexHome = codexHome
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try installBundle(source: source, destination: destination)
                var skillMessage: String?
                if includeSkills {
                    do {
                        let backup = try installSkills(source: destination.appendingPathComponent("Contents/Resources/dj-skills"), codexHome: codexHome)
                        skillMessage = "Three DJ skills installed. Restart Codex to discover them." + (backup.map { " Previous skills saved in \($0.path)." } ?? "")
                    } catch {
                        skillMessage = "PulseDeck installed; skills could not be installed: \(error.localizedDescription)"
                    }
                }
                let message = skillMessage
                DispatchQueue.main.async { self.copying = false; self.installed = true; self.skillMessage = message }
            } catch {
                DispatchQueue.main.async { self.copying = false; self.error = "Could not install: \(error.localizedDescription)" }
            }
        }
    }
}

// A native drag session supplies a bounded preview and always resets on cancellation.
struct DragIcon: NSViewRepresentable {
    let model: InstallModel
    func makeNSView(context: Context) -> DragImage {
        let view = DragImage(); view.model = model
        view.image = NSImage(contentsOf: Bundle.main.url(forResource:"AppIcon",withExtension:"png")!)
        view.image?.size = NSSize(width:108,height:108)
        view.imageScaling = .scaleProportionallyUpOrDown
        view.setContentCompressionResistancePriority(.defaultLow,for:.horizontal)
        view.setContentCompressionResistancePriority(.defaultLow,for:.vertical)
        view.wantsLayer = true; view.layer?.cornerRadius = 22; view.layer?.masksToBounds = true
        return view
    }
    func updateNSView(_ view: DragImage, context: Context) { view.alphaValue = model.dragging ? 0.3 : 1 }
}
final class DragImage: NSImageView, NSDraggingSource {
    override var intrinsicContentSize: NSSize { NSSize(width:108,height:108) }
    weak var model: InstallModel?
    var active = false
    override var mouseDownCanMoveWindow: Bool { false }
    override func mouseDown(with event: NSEvent) {}
    override func mouseDragged(with event: NSEvent) {
        guard let model = model, !active, !model.copying, !model.installed, let image = image else { return }
        active = true; model.dragging = true
        let item = NSDraggingItem(pasteboardWriter:model.source as NSURL)
        // AppKit receives the same rounded silhouette as the source, with room for its shadow.
        let preview = NSImage(size:NSSize(width:124,height:124),flipped:false) { _ in
            let rect = NSRect(x:8,y:8,width:108,height:108)
            NSGraphicsContext.saveGraphicsState()
            let shadow = NSShadow(); shadow.shadowColor = NSColor.black.withAlphaComponent(0.2)
            shadow.shadowBlurRadius = 5; shadow.shadowOffset = NSSize(width:0,height:-2); shadow.set()
            NSColor.white.setFill(); NSBezierPath(roundedRect:rect,xRadius:22,yRadius:22).fill()
            NSGraphicsContext.restoreGraphicsState()
            NSGraphicsContext.saveGraphicsState()
            NSBezierPath(roundedRect:rect,xRadius:22,yRadius:22).addClip()
            image.draw(in:rect)
            NSGraphicsContext.restoreGraphicsState()
            return true
        }
        alphaValue = 0.3
        item.setDraggingFrame(bounds.insetBy(dx:-8,dy:-8),contents:preview)
        let session = beginDraggingSession(with:[item],event:event,source:self)
        session.animatesToStartingPositionsOnCancelOrFail = true
    }
    func draggingSession(_ session: NSDraggingSession, sourceOperationMaskFor context: NSDraggingContext) -> NSDragOperation { .copy }
    func draggingSession(_ session: NSDraggingSession, movedTo screenPoint: NSPoint) {
        if let window = window { model?.pointer = window.convertPoint(fromScreen:screenPoint) }
    }
    func draggingSession(_ session: NSDraggingSession, endedAt screenPoint: NSPoint, operation: NSDragOperation) {
        active = false; alphaValue = 1; model?.dragging = false; model?.hovering = false
    }
}

struct InstallDrop: DropDelegate {
    let model: InstallModel
    let target: Bool
    func validateDrop(info: DropInfo) -> Bool { info.hasItemsConforming(to: [.fileURL]) && !model.copying && !model.installed }
    func dropEntered(info: DropInfo) { model.dragging = true; if target { model.hovering = true } }
    func dropExited(info: DropInfo) { if target { model.hovering = false } else { model.dragging = false } }
    func dropUpdated(info: DropInfo) -> DropProposal? {
        if !target { model.pointer = info.location }
        return DropProposal(operation: target ? .copy : .cancel)
    }
    func performDrop(info: DropInfo) -> Bool {
        guard target, let item = info.itemProviders(for: [.fileURL]).first else { model.dragging = false; model.hovering = false; return false }
        _ = item.loadObject(ofClass: URL.self) { url, _ in
            DispatchQueue.main.async {
                model.dragging = false; model.hovering = false
                guard url?.standardizedFileURL == model.source.standardizedFileURL else {
                    model.error = "Drag the PulseDeck icon from this installer into Applications."
                    return
                }
                model.requestInstall()
            }
        }
        return true
    }
}

let signal = Color(red:1,green:90/255,blue:31/255)
let ink = Color(red:24/255,green:25/255,blue:27/255)
func ui(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font { .custom("HankenGrotesk-Regular",size:size).weight(weight) }
func mono(_ size: CGFloat) -> Font { .custom("DMMono-Regular",size:size) }

struct InstallerView: View {
    @ObservedObject var model: InstallModel
    @Environment(\.accessibilityReduceMotion) var reduceMotion
    var progress: Double { model.hovering ? 1 : min(1,max(0,(model.pointer.x-240)/280)) }
    var body: some View {
        VStack(spacing:0) {
            HStack(spacing:10) {
                BrandDots().frame(width:22,height:22)
                Text("pulsedeck").font(ui(20,.medium)).tracking(-0.4)
                Spacer()
                Circle().fill(signal).frame(width:5,height:5)
                Text("INSTALL / macOS").font(mono(10)).tracking(1).foregroundColor(ink.opacity(0.6))
            }.padding(.horizontal,28).frame(height:70)
            Divider().overlay(Color.black.opacity(0.12))
            VStack(alignment:.leading,spacing:0) {
                HStack(alignment:.firstTextBaseline) {
                    Text(model.installed ? "Ready to play." : "Install PulseDeck").font(ui(30,.medium)).tracking(-0.8)
                    Spacer()
                    Text(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "").font(mono(11)).foregroundColor(ink.opacity(0.5))
                }
                Text(model.installed ? "The application is in Applications." : "Drag the app across, or use the install button.")
                    .font(ui(14)).foregroundColor(ink.opacity(0.6)).padding(.top,6)
                VStack(spacing:0) {
                    HStack(spacing:32) {
                        VStack(spacing:12) {
                            DragIcon(model:model).frame(width:108,height:108).clipped()
                                .shadow(color:.black.opacity(0.15),radius:4,y:3)
                                .accessibilityLabel("PulseDeck application. Drag into Applications to install.")
                            Text("PulseDeck.app").font(mono(11))
                        }
                        VStack(spacing:8) {
                            ZStack {
                                InstallCurve().stroke(ink.opacity(0.15),style:StrokeStyle(lineWidth:2,lineCap:.round))
                                InstallCurve().trim(from:0,to:model.installed ? 1 : model.dragging ? 0.08+0.92*progress : 0.04)
                                    .stroke(signal,style:StrokeStyle(lineWidth:2,lineCap:.round))
                            }.frame(width:120,height:38)
                            Text(model.copying ? "INSTALLING" : model.installed ? "INSTALLED" : model.hovering ? "RELEASE" : model.dragging ? "DRAG ACROSS" : "→")
                                .font(mono(9)).foregroundColor(ink.opacity(0.45))
                        }.accessibilityHidden(true)
                        VStack(spacing:12) {
                            ZStack {
                                RoundedRectangle(cornerRadius:22).fill(LinearGradient(colors:[Color.white.opacity(model.hovering ? 0.8 : 0.6),Color.black.opacity(0.035)],startPoint:.top,endPoint:.bottom))
                                RoundedRectangle(cornerRadius:22).stroke(model.hovering ? signal : ink.opacity(0.22),lineWidth:model.hovering ? 2 : 1)
                                Image(systemName:model.installed ? "checkmark" : "folder")
                                    .font(.system(size:44,weight:.light)).foregroundColor(model.hovering || model.installed ? signal : ink.opacity(0.7))
                            }.frame(width:108,height:108)
                                .scaleEffect(model.hovering && !reduceMotion ? 0.97 : 1)
                                .onDrop(of:[.fileURL],delegate:InstallDrop(model:model,target:true))
                                .accessibilityLabel("Applications install target")
                            Text("Applications").font(mono(11))
                        }
                    }.frame(maxWidth:.infinity).padding(.vertical,24)
                }.padding(.top,30).padding(.bottom,6)
                if !model.installed {
                    Toggle("Install/update DJ skills for Codex",isOn:$model.includeSkills).toggleStyle(.checkbox)
                        .font(ui(13)).disabled(model.copying).padding(.top,20)
                    Text("Three skills. Existing versions are backed up before replacement.")
                        .font(ui(11)).foregroundColor(ink.opacity(0.55)).padding(.top,4)
                }
                if let message = model.skillMessage { Text(message).font(ui(11)).foregroundColor(ink.opacity(0.7)).padding(.top,12).fixedSize(horizontal:false,vertical:true) }
                if let error = model.error { Text(error).font(ui(12)).foregroundColor(Color(red:0.7,green:0.17,blue:0.02)).padding(.top,10).fixedSize(horizontal:false,vertical:true) }
                Spacer(minLength:16)
                HStack {
                    Text(model.destination.deletingLastPathComponent().path).font(mono(10)).foregroundColor(ink.opacity(0.5))
                    Spacer()
                    if model.copying {
                        ProgressView().controlSize(.small)
                        Text("Installing…").font(ui(13))
                    } else if model.installed {
                        Button("Open PulseDeck") { NSWorkspace.shared.openApplication(at:model.destination,configuration:NSWorkspace.OpenConfiguration()) }
                            .buttonStyle(.borderedProminent).tint(ink).keyboardShortcut(.defaultAction)
                    } else {
                        Button("Install PulseDeck") { model.requestInstall() }
                            .buttonStyle(.borderedProminent).tint(ink).keyboardShortcut(.defaultAction)
                    }
                }.font(ui(13,.medium))
            }.padding(28)
        }.foregroundColor(ink).frame(width:720,height:560)
            .background(LinearGradient(colors:[Color(red:233/255,green:234/255,blue:236/255),Color(red:196/255,green:197/255,blue:200/255)],startPoint:.topLeading,endPoint:.bottomTrailing))
            .preferredColorScheme(.light)
            .animation(reduceMotion ? nil : .interactiveSpring(response:0.34,dampingFraction:0.76),value:model.hovering)
            .onChange(of:model.copying) { copying in
                NSApp.windows.first { $0.title == "Install PulseDeck" }?.standardWindowButton(.closeButton)?.isEnabled = !copying
            }
            .alert("Replace PulseDeck?",isPresented:$model.confirmReplace) {
                Button("Replace") { model.install() }; Button("Cancel",role:.cancel) {}
            } message: { Text("The installed application will be replaced. Your music, library and account settings stay in their own folders.") }
    }
}
struct InstallCurve: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to:CGPoint(x:0,y:rect.midY))
        path.addCurve(to:CGPoint(x:rect.maxX,y:rect.midY),control1:CGPoint(x:rect.width*0.32,y:rect.height*0.05),control2:CGPoint(x:rect.width*0.68,y:rect.height*0.95))
        return path
    }
}
struct BrandDots: View {
    var body: some View {
        GeometryReader { box in
            ForEach(0..<25) { i in
                let x = 2.2+Double(i%5)*4.4, y = 2.2+Double(i/5)*4.4
                if hypot(x-11,y-11) < 10.5 {
                    Circle().fill(i==12 ? signal : ink)
                        .frame(width:box.size.width*2.9/22,height:box.size.height*2.9/22)
                        .position(x:box.size.width*x/22,y:box.size.height*y/22)
                }
            }
        }
    }
}
final class InstallerDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    let model=InstallModel(); var window:NSWindow!
    func applicationDidFinishLaunching(_ notification:Notification) {
        let menu = NSMenu(), applicationMenu = NSMenu(), item = NSMenuItem()
        applicationMenu.addItem(withTitle:"Quit Install PulseDeck",action:#selector(NSApplication.terminate(_:)),keyEquivalent:"q")
        item.submenu = applicationMenu; menu.addItem(item); NSApp.mainMenu = menu
        window=NSWindow(contentRect:NSRect(x:0,y:0,width:720,height:560),styleMask:[.titled,.closable,.miniaturizable,.fullSizeContentView],backing:.buffered,defer:false)
        window.title="Install PulseDeck"; window.titleVisibility = .hidden; window.titlebarAppearsTransparent=true
        window.isMovableByWindowBackground=false; window.acceptsMouseMovedEvents=true; window.delegate=self
        window.contentView=NSHostingView(rootView:InstallerView(model:model)); window.center(); window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps:true)
    }
    func applicationShouldTerminate(_ sender:NSApplication) -> NSApplication.TerminateReply {
        model.copying ? .terminateCancel : .terminateNow
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender:NSApplication)->Bool { true }
}

for name in ["HankenGrotesk", "DMMono-Regular"] {
    if let url = Bundle.main.url(forResource:name,withExtension:"ttf") { CTFontManagerRegisterFontsForURL(url as CFURL,.process,nil) }
}
if CommandLine.arguments.contains("--self-test") {
    if Bundle.main.url(forResource:"HankenGrotesk",withExtension:"ttf") != nil {
        precondition(NSFont(name:"HankenGrotesk-Regular",size:14) != nil)
        precondition(NSFont(name:"DMMono-Regular",size:14) != nil)
    }
    let fm=FileManager.default
    let root=fm.temporaryDirectory.appendingPathComponent("pulsedeck-installer-\(UUID().uuidString)")
    defer { try? fm.removeItem(at:root) }
    let source=root.appendingPathComponent("source/PulseDeck.app"), destination=root.appendingPathComponent("Applications/PulseDeck.app")
    try fm.createDirectory(at:source.appendingPathComponent("Contents"),withIntermediateDirectories:true)
    let info:[String:Any] = ["CFBundleIdentifier":"app.pulsedeck.desktop"]
    try PropertyListSerialization.data(fromPropertyList:info,format:.xml,options:0).write(to:source.appendingPathComponent("Contents/Info.plist"))
    try Data("first".utf8).write(to:source.appendingPathComponent("version"))
    try installBundle(source:source,destination:destination)
    precondition((try? String(contentsOf:destination.appendingPathComponent("version"),encoding:.utf8))=="first")
    try Data("second".utf8).write(to:source.appendingPathComponent("version"))
    try installBundle(source:source,destination:destination)
    precondition((try? String(contentsOf:destination.appendingPathComponent("version"),encoding:.utf8))=="second")
    try fm.removeItem(at:source)
    do { try installBundle(source:source,destination:destination); fatalError("Missing source accepted") } catch {}
    precondition((try? String(contentsOf:destination.appendingPathComponent("version"),encoding:.utf8))=="second")
    let skills = root.appendingPathComponent("bundled-skills"), codexHome = root.appendingPathComponent("codex")
    for name in skillNames {
        let folder = skills.appendingPathComponent(name)
        try fm.createDirectory(at:folder,withIntermediateDirectories:true)
        try Data("first".utf8).write(to:folder.appendingPathComponent("SKILL.md"))
    }
    let firstBackup = try installSkills(source:skills,codexHome:codexHome)
    precondition(firstBackup == nil)
    let oldSkill = codexHome.appendingPathComponent("skills/pulsedeck-control/SKILL.md")
    try Data("custom".utf8).write(to:oldSkill)
    let backup = try installSkills(source:skills,codexHome:codexHome)!
    precondition((try? String(contentsOf:backup.appendingPathComponent("pulsedeck-control/SKILL.md"),encoding:.utf8)) == "custom")
    precondition((try? String(contentsOf:oldSkill,encoding:.utf8)) == "first")
    try fm.removeItem(at:skills.appendingPathComponent("djing-art"))
    do { _ = try installSkills(source:skills,codexHome:codexHome); fatalError("Incomplete skills accepted") } catch {}
    precondition((try? String(contentsOf:oldSkill,encoding:.utf8)) == "first")
    print("Installer app copy/replacement, skill install/update/backup and failure preservation passed in temporary folders.")
} else {
    let application=NSApplication.shared; let delegate=InstallerDelegate(); application.delegate=delegate
    application.setActivationPolicy(.regular); application.run()
}
