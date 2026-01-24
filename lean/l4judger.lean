import Batteries.Tactic.OpenPrivate
import CollectLiterals
import Lean.Compiler.ImplementedByAttr
import Lean.Elab.DefView
import Lean.Elab.Import
import Lean.Language.Lean
import Lean.Replay

open Lean Elab Language

def setup (stx : HeaderSyntax) : ProcessingT IO (Except Lean.HeaderProcessedSnapshot Lean.SetupImportsResult) := do
  pure <| .ok {
    mainModuleName := `Lean4OJ.main,
    isModule := stx.isModule,
    imports := stx.imports,
    opts := {},
    trustLevel := 0,
  }

def Lean.Name.isStd : Name → Bool
  | .str .anonymous "Aesop" => true
  | .str .anonymous "Archive" => true
  | .str .anonymous "Batteries" => true
  | .str .anonymous "Counterexamples" => true
  | .str .anonymous "ImportGraph" => true
  | .str .anonymous "Init" => true
  | .str .anonymous "Lake" => true
  | .str .anonymous "Lean" => true
  | .str .anonymous "LeanSearchClient" => true
  | .str .anonymous "Mathlib" => true
  | .str .anonymous "Plausible" => true
  | .str .anonymous "ProofWidgets" => true
  | .str .anonymous "Qq" => true
  | .str .anonymous "Std" => true
  | .str .anonymous "docs" => true
  | .str .anonymous "references" => true
  | .str .anonymous "Lean4OJ" => true
  | _ => false

open private Lean.Kernel.Environment.mk Lean.Kernel.Environment.extensions Lean.Kernel.Environment.irBaseExts from Lean.Environment in
def reduceEnv (env : Environment) (names : Std.HashSet Name) : Environment :=
  let inner := env.toKernelEnv
  let consts := inner.constants.map₁.filter fun k _ => !names.contains k
  let consts' : ConstMap := Lean.SMap.fromHashMap consts
  let env' := Lean.Kernel.Environment.mk consts' inner.quotInit inner.diagnostics inner.const2ModIdx (Lean.Kernel.Environment.extensions inner) (Lean.Kernel.Environment.irBaseExts inner) inner.header
  Environment.ofKernelEnv env'

def kitsune (n : Nat) : ByteArray :=
  let b0 : UInt8 := UInt8.ofNat (n &&& 0xFF)
  let b1 : UInt8 := UInt8.ofNat ((n >>> 8) &&& 0xFF)
  let b2 : UInt8 := UInt8.ofNat ((n >>> 16) &&& 0xFF)
  let b3 : UInt8 := UInt8.ofNat ((n >>> 24) &&& 0xFF)
  .mk #[b0, b1, b2, b3]

inductive JudgeStatus
  | JudgerReceived
  | TypeChecking
  | AxiomChecking
  | Replaying
  | WrongAnswer
  | Accepted
  | JudgementFailed

def JudgeStatus.toByte : JudgeStatus → UInt8
  | .JudgerReceived  => 3
  | .TypeChecking    => 4
  | .AxiomChecking   => 5
  | .Replaying       => 6
  | .WrongAnswer     => 8
  | .Accepted        => 9
  | .JudgementFailed => 10

inductive MessageAction
  | NoAction
  | Replace (s : String)
  | Append  (s : String)

def MessageAction.toBytes : MessageAction → ByteArray
  | .NoAction  => .mk #[0]
  | .Replace s => .mk #[1] ++ kitsune s.utf8ByteSize ++ s.toByteArray
  | .Append s  => .mk #[2] ++ kitsune s.utf8ByteSize ++ s.toByteArray

def reportRaw (s : ByteArray) : IO Unit := do
  let stdout ← IO.getStdout
  stdout.write s
  stdout.flush

def report (status : JudgeStatus) (action : MessageAction) (answer : Option String) : IO Unit :=
  reportRaw <|
    .mk #[status.toByte] ++
    action.toBytes ++
    match answer with
    | some ans => (.mk #[1] ++ kitsune ans.utf8ByteSize ++ ans.toByteArray)
    | none => (.mk #[0])

def handleMessage (tot : Nat) (msg : Message) : IO Nat := do
  let cnt := match msg.severity with
    | .error => 1
    | _ => 0
  let e ← msg.toString true
  report .TypeChecking (.Append e) none
  pure (tot + cnt)

def extractAnswer : Option ConstantInfo → Option String
  | some ci =>
    match ci.value? with
    | some (.lit (.strVal s)) => some s
    | _ => none
  | none => none

def main (args : List String) : IO Unit := do
  let fileName::allowedAxiomsList := args | return
  searchPathRef.set (← addSearchPathFromEnv [])
  let allowedAxioms := Std.HashSet.ofList (allowedAxiomsList.map String.toName)

  let contents : String ←
    try
      IO.FS.readFile fileName
    catch e =>
      report .JudgementFailed (.Replace e.toString) none
      return
  let ctx := Parser.mkInputContext contents "main.lean"

  report .TypeChecking (.Replace "") none

  let snap : InitialSnapshot ←
    try
      Lean.process setup none ⟨ctx⟩
    catch e =>
      report .WrongAnswer (.Replace e.toString) none
      return
  let snaps := Language.toSnapshotTree snap

  let errMonad : IO Nat := snaps.foldM (fun (tot : Nat) (snap : Snapshot) => snap.diagnostics.msgLog.unreported.foldlM handleMessage tot) 0
  let err : Nat ←
    try
      errMonad
    catch e =>
      report .WrongAnswer (.Replace e.toString) none
      return
  if err > 0 then
    report .WrongAnswer MessageAction.NoAction none
    return

  let some (cmdState : Command.State) := Language.Lean.waitForFinalCmdState? snap |
    report .JudgementFailed (.Append "Failed to obtain final command state") none
    return

  let some answer := extractAnswer (cmdState.env.find? `Lean4OJ.answer_str) |
    report .WrongAnswer (.Append "Failed to extract answer") none
    return

  let (malform, state) := CollectLiterals.collect `Lean4OJ.answer cmdState.env {}
  if malform then
    report .WrongAnswer (.Append "Malformed module") none
    return

  report .AxiomChecking .NoAction (some answer)

  let (_, state) := CollectAxioms.collect `Lean4OJ.answer cmdState.env {}
  for Axiom in state.axioms do
    if !(allowedAxioms.contains Axiom) then
      report .WrongAnswer (.Append s!"Disallowed axiom: {Axiom}.") none
      return

  report .Replaying .NoAction none

  let mut consts : Std.HashMap Name ConstantInfo := {}
  for mod in cmdState.env.header.modules, data in cmdState.env.header.moduleData, idx in (*...* : Std.Rii Nat) do
    if !mod.module.getRoot.isStd then
      for name in data.constNames, ci in data.constants do
        consts := consts.insert name ci
    else
      for name in data.constNames, ci in data.constants do
        let idx := cmdState.env.const2ModIdx.get? name
        let eff := idx.bind (fun i => cmdState.env.header.modules[i]?)
        let clean := match eff with
          | some e => e.module.getRoot.isStd
          | none => false
        if !clean then
          consts := consts.insert name ci
  for (name, _) in consts do
    if let some name' := Lean.Compiler.getImplementedBy? cmdState.env name then
      report .WrongAnswer (.Append s!"Use of implemented-by {name} => {name'} is disallowed.") none
      return

  let replayMonad := (reduceEnv cmdState.env <| Std.HashSet.ofList consts.keys).replay consts
  try
    replayMonad
  catch e =>
    report .WrongAnswer (.Append e.toString) none
    return

  report .Accepted .NoAction none
