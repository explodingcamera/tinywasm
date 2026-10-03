use alloc::{boxed::Box, vec::Vec};

use crate::{Export, Module};

/// Core modules and declarations from a component-model Wasm file.
#[derive(Clone, Default, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct Component {
    /// Core modules defined directly in this component.
    pub modules: Vec<Module>,
    /// Declarations, including imports and aliases that introduce indices.
    pub declarations: Vec<ComponentDeclaration>,
}

/// A declaration in a component scope.
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentDeclaration {
    /// A declaration in the core module index space.
    CoreModule(ComponentCoreModule),
    /// A declaration in the component index space.
    Component(ComponentIndex),
    /// A core instance, in the core instance index space.
    CoreInstance(ComponentCoreInstance),
    /// A component export referring to an entry in an index space.
    Export(ComponentExport),
    /// An import in a component index space other than core modules or components.
    Import { name: ComponentName, ty: ComponentTypeRef },
    /// A component instance in its index space.
    Instance(ComponentInstance),
    /// Alias in one of the remaining component index spaces.
    Alias(ComponentAlias),
    /// The component start function.
    Start(ComponentStart),
    /// A core type definition in the core type index space.
    CoreType(ComponentCoreType),
    /// A component type definition in the component type index space.
    Type(ComponentType),
    /// A canonical function or intrinsic, in its respective index space.
    Canonical(CanonicalFunction),
}

/// A type defined by a component.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentType {
    /// A defined value type.
    Defined(ComponentDefinedType),
    /// A component function type.
    Func { async_: bool, params: Vec<(Box<str>, ComponentValueType)>, result: Option<ComponentValueType> },
    /// A component type with declarations in its own index spaces.
    Component(Vec<ComponentTypeDeclaration>),
    /// A component instance type with declarations in its own index spaces.
    Instance(Vec<ComponentTypeDeclaration>),
    /// A resource represented by a core value and optional destructor function.
    Resource { rep: crate::WasmType, dtor: Option<u32> },
}

/// A declaration inside a component or instance type.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentTypeDeclaration {
    /// A core type.
    CoreType(ComponentCoreType),
    /// A component type.
    Type(ComponentType),
    /// An alias in a type's index spaces.
    Alias(ComponentAlias),
    /// A named import.
    Import { name: ComponentName, ty: ComponentTypeRef },
    /// A named export.
    Export { name: ComponentName, ty: ComponentTypeRef },
}

/// A defined component value type.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentDefinedType {
    /// A primitive.
    Primitive(ComponentPrimitiveType),
    /// Named fields.
    Record(Vec<(Box<str>, ComponentValueType)>),
    /// Named cases with optional payloads.
    Variant(Vec<(Box<str>, Option<ComponentValueType>)>),
    /// A list.
    List(ComponentValueType),
    /// A map from keys to values.
    Map(ComponentValueType, ComponentValueType),
    /// A fixed-length list.
    FixedLengthList(ComponentValueType, u32),
    /// A tuple.
    Tuple(Vec<ComponentValueType>),
    /// Named flags.
    Flags(Vec<Box<str>>),
    /// Named enum cases.
    Enum(Vec<Box<str>>),
    /// An optional value.
    Option(ComponentValueType),
    /// A result with optional success and error payloads.
    Result { ok: Option<ComponentValueType>, err: Option<ComponentValueType> },
    /// An owned resource handle.
    Own(u32),
    /// A borrowed resource handle.
    Borrow(u32),
    /// A future with an optional payload.
    Future(Option<ComponentValueType>),
    /// A stream with an optional payload.
    Stream(Option<ComponentValueType>),
}

/// A core type defined in a component.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentCoreType {
    /// A recursive group of existing core subtypes.
    Rec(crate::TypeSection),
    /// A core module type with its own type index space.
    Module(Vec<CoreModuleTypeDeclaration>),
}

/// A declaration in a core module type.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum CoreModuleTypeDeclaration {
    /// A recursive group of existing core subtypes.
    Type(crate::TypeSection),
    /// An import with an existing core import type.
    Import(crate::Import),
    /// An export with a core type reference.
    Export { name: Box<str>, ty: crate::ImportKind },
    /// A core type alias from an outer scope.
    OuterAlias { count: u32, index: u32 },
}

/// A component start function and its value arguments.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentStart {
    /// Component function index.
    pub func: u32,
    /// Indices of the value arguments.
    pub arguments: Vec<u32>,
    /// Expected number of results.
    pub results: u32,
}

/// A component alias outside the core-module and component index spaces.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentAlias {
    /// Alias to an export from a component instance.
    InstanceExport { kind: ComponentExternalKind, instance: u32, name: Box<str> },
    /// Alias to an export from a core instance.
    CoreInstanceExport { kind: crate::ExternalKind, instance: u32, name: Box<str> },
    /// Alias to an item in an outer component scope.
    Outer { kind: ComponentOuterAliasKind, count: u32, index: u32 },
}

/// The index space of an outer alias.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentOuterAliasKind {
    /// Core module.
    CoreModule,
    /// Core type.
    CoreType,
    /// Component type.
    Type,
    /// Nested component.
    Component,
}

/// A component instance declaration.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentInstance {
    /// Instantiate a nested component.
    Instantiate { component: u32, args: Vec<ComponentInstanceArg> },
    /// Bundle component exports into an instance.
    FromExports(Vec<ComponentExport>),
}

/// An argument when instantiating a component.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentInstanceArg {
    /// Name of the component import.
    pub name: Box<str>,
    /// Index space of the argument.
    pub kind: ComponentExternalKind,
    /// Index of the supplied item.
    pub index: u32,
}

/// A component export and its optional ascribed type.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentExport {
    /// Exported name and annotations.
    pub name: ComponentName,
    /// Index space of the exported item.
    pub kind: ComponentExternalKind,
    /// Index of the exported item.
    pub index: u32,
    /// Optional type ascription.
    pub ty: Option<ComponentTypeRef>,
}

/// The component index space of an external item.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentExternalKind {
    /// Core module.
    Module,
    /// Component function.
    Func,
    /// Component value.
    Value,
    /// Component type.
    Type,
    /// Component instance.
    Instance,
    /// Nested component.
    Component,
}

/// A reference to a component import or export type.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentTypeRef {
    /// Core module type index.
    Module(u32),
    /// Component function type index.
    Func(u32),
    /// Component value type.
    Value(ComponentValueType),
    /// Defined type index.
    Type(u32),
    /// A subtype of the resource bound.
    SubResource,
    /// Component instance type index.
    Instance(u32),
    /// Component type index.
    Component(u32),
}

/// A value type in a component import or export.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentValueType {
    /// Defined type index.
    Type(u32),
    /// Primitive component value type.
    Primitive(ComponentPrimitiveType),
}

/// Primitive component value types.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentPrimitiveType {
    /// Boolean.
    Bool,
    /// Signed 8-bit integer.
    S8,
    /// Unsigned 8-bit integer.
    U8,
    /// Signed 16-bit integer.
    S16,
    /// Unsigned 16-bit integer.
    U16,
    /// Signed 32-bit integer.
    S32,
    /// Unsigned 32-bit integer.
    U32,
    /// Signed 64-bit integer.
    S64,
    /// Unsigned 64-bit integer.
    U64,
    /// 32-bit floating-point number.
    F32,
    /// 64-bit floating-point number.
    F64,
    /// Unicode character.
    Char,
    /// String.
    String,
    /// Error context.
    ErrorContext,
}

/// A core instance declaration.
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentCoreInstance {
    /// Instantiate a core module using core instance arguments.
    Instantiate { module: u32, args: Vec<ComponentInstantiationArg> },
    /// Bundle core exports into an instance.
    FromExports(Vec<Export>),
}

/// A core instance argument passed when instantiating a module.
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentInstantiationArg {
    /// Name of the module import.
    pub name: Box<str>,
    /// Index of the core instance supplied for the import.
    pub instance: u32,
}

/// An entry in a component's core module index space.
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentCoreModule {
    /// An embedded module, indexed into [`Component::modules`].
    Defined(usize),
    /// A module imported by name, with a core type index.
    Import { name: ComponentName, ty: u32 },
    /// A module aliased from a component instance export.
    InstanceExport { instance: u32, name: Box<str> },
    /// A module aliased from an outer component scope.
    OuterAlias { count: u32, index: u32 },
}

/// An entry in a component's component index space.
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ComponentIndex {
    /// A nested component with its own index spaces.
    Defined(Box<Component>),
    /// A component imported by name, with a component type index.
    Import { name: ComponentName, ty: u32 },
    /// A component aliased from a component instance export.
    InstanceExport { instance: u32, name: Box<str> },
    /// A component aliased from an outer component scope.
    OuterAlias { count: u32, index: u32 },
}

/// A component import or export name and its optional annotations.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentName {
    /// Declared name.
    pub name: Box<str>,
    /// Interface implemented by this declaration.
    pub implements: Option<Box<str>>,
    /// Suffix for a versioned interface name.
    pub version_suffix: Option<Box<str>>,
    /// External identifier.
    pub external_id: Option<Box<str>>,
}

/// An option controlling canonical ABI conversion.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum CanonicalOption {
    /// The string types in the function signature are UTF-8 encoded.
    UTF8,
    /// The string types in the function signature are UTF-16 encoded.
    UTF16,
    /// The string types in the function signature are compact UTF-16 encoded.
    CompactUTF16,
    /// The memory to use if the lifting or lowering of a function requires memory access.
    ///
    /// The value is an index to a core memory.
    Memory(u32),
    /// The realloc function to use if the lifting or lowering of a function requires memory
    /// allocation.
    ///
    /// The value is an index to a core function of type `(func (param $T $T $T $T) (result $T))` where
    /// `$T` is the index type of the memory, i.e., either `i32` or `i64`.
    Realloc(u32),
    /// The post-return function to use if the lifting of a function requires
    /// cleanup after the function returns.
    PostReturn(u32),
    /// Indicates that specified function should be lifted or lowered using the `async` ABI.
    Async,
    /// The function to use if the async lifting of a function should receive task/stream/future progress events
    /// using a callback.
    Callback(u32),
    /// The core function type to lower this component function to.
    CoreType(u32),
    /// Use the GC version of the canonical ABI.
    Gc,
}

/// A canonical ABI function or intrinsic declaration.
///
/// `name` identifies the operation, for example `lift` or `stream.read`.
/// Operand order follows the named operation. Indices use the index space
/// specified by that operation's operand, such as a core function for `lift`.
/// `lift` takes a core function and a component function type index. `lower`
/// takes a component function index. `context.get` and `context.set` take a core
/// value type and a slot. The thread spawn and new-indirect operations take a
/// function type and optionally a table index. Stream and future cancellation
/// operations take a type index and an async flag. `task.return` takes an
/// optional component result type. Other operations take the indices indicated
/// by their names, or no arguments.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct CanonicalFunction {
    /// Canonical operation name.
    pub name: Box<str>,
    /// Operands in the order defined by the named operation.
    pub args: Vec<CanonicalArgument>,
    /// Canonical ABI options in declaration order.
    pub options: Vec<CanonicalOption>,
}

/// A positional canonical operand. Its meaning depends on the operation name.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum CanonicalArgument {
    /// An index in the operation-specific index space.
    Index(u32),
    /// A core Wasm value type.
    CoreValueType(crate::WasmType),
    /// An optional component value type.
    ResultType(Option<ComponentValueType>),
    /// A boolean flag.
    Bool(bool),
}
