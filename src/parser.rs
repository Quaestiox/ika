use crate::{lexer::{tokenization, Error, Token, TokenType, LEXER}, sema::{current_index, get_fun, get_var, has_var, insert_var,get_ty}};
use std::{collections::HashMap, fmt::format};
use crate::sema::{SYMBOL_TABLES,Function};

#[derive(Debug, Clone,PartialEq)]
#[allow(dead_code)]
pub enum ASTNode {
    Program(Vec<ASTNode>),
    Assignment{
        identifier: String,
        var_value: Option<Box<ASTNode>>,
    },
    FunctionDefinition{
        fn_name: String,
        parameters:Vec<(String, String)>,
        ret_type: Option<String>,
        body: Vec<ASTNode>
    },
    VariableDefinition{
        var_type: String,
        identifier: String,
        var_value: Option<Box<ASTNode>>,
    },
    FunctionCall{
        fn_name:String,
        argument: Vec<ASTNode>,
    },
    InfixExpression{
        left_expr: Box<ASTNode>,
        op: String,
        right_expr:Box<ASTNode>,
    },
    IfElse{
        condition: Box<ASTNode>,
        if_body: Vec<ASTNode>,
        elif_body:  Vec<Vec<ASTNode>>,
        el_condition: Vec<ASTNode>,
        else_body: Option<Vec<ASTNode>>,
    },
    While{
        condition: Box<ASTNode>,
        body: Vec<ASTNode>,
    },
    Return(Box<ASTNode>),
    Expression(Box<ASTNode>),
    Number(String),
    String(String),
    Identifier(String),
}

#[derive(Debug)]
pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    fn peek(&self) -> Result<&Token, Error> {
        if self.current < self.tokens.len() {
            Ok(&self.tokens[self.current])
        } else {
            Err(Error::ParserErr)
        }

    }
    
    fn advance(&mut self) -> Result<&Token, Error> {
        if self.current < self.tokens.len() {
            self.current += 1;
            Ok(&self.tokens[self.current-1])
        } else {
            Err(Error::ParserErr)
        }
    }

    fn expect(&mut self, ty:TokenType, value: String) -> Result<(),String>{
        let c = self.advance().unwrap();
        if c.token_type == ty && c.value == value {
            Ok(())
        }else{
            Err(format!("Expected '{:?}' {:?}, found '{:?}' {:?}", ty, value, c.token_type, c.value))
        }
    }

    pub fn parse_program(&mut self) -> Result<ASTNode, String>{
        let mut statements:Vec<ASTNode> = Vec::new();
        while self.peek().unwrap().token_type != TokenType::EOF{
            statements.push(self.parse_statement()?);
        }
        Ok(ASTNode::Program(statements))
    }

    fn parse_statement(&mut self) -> Result<ASTNode, String>{
        let token = self.peek().unwrap();
        match token.token_type{
            TokenType::KEYWORD => {
                match token.value.as_str() {
                    "sub" => self.parse_function_definition(),
                    "ret" => self.parse_return(),
                    "i32" | "str" => self.parse_variable_definition(),
                    "while" => self.parse_while(),
                    "if" => self.parse_if_else(),
                    _ => Err(format!("parse_statement error"))
                }
            }
            TokenType::ID => {
                let token = self.advance().unwrap().clone();
                let cur = self.peek().unwrap();
                match cur.token_type{
                    TokenType::EQUALS => {
                        self.parse_assignment(token.value.clone())
                    }
                    TokenType::LPAREN =>{
                        
                        let res = self.parse_function_call(token.value.clone());
                        if res.is_err(){
                            Ok(res?.0)
                        }else{
                            self.expect(TokenType::SEMICOLON, String::from(";"))?;
                            Ok(res?.0)
                        }
                
                    }
                    _ => Err(format!("Invalid symbol {:?}",cur))
                }
            }
            _ => {
                let ex = self.parse_expression();
                Ok(ex?.0)

            }

        }
    }

    fn parse_function_definition(&mut self) -> Result<ASTNode, String>{
        self.expect(TokenType::KEYWORD, String::from("sub"))?;
        let fn_name = handle_identifier(self.advance().unwrap().value.as_str())?;
        if SYMBOL_TABLES.lock().unwrap().current_scope().has_function(fn_name.as_str()) {
            return Err(format!("Function '{}' is already defined", fn_name));
        }
        self.expect(TokenType::LPAREN, String::from("("))?;
        let mut parameters = Vec::new();
        while self.peek().unwrap().token_type != TokenType::RPAREN{  
            let para_type = handle_type(self.advance().unwrap().value.as_str())?;
            let para_name = handle_identifier(self.advance().unwrap().value.as_str())?;
            parameters.push((para_type, para_name));
            if self.peek().unwrap().token_type == TokenType::COMMA{
                self.advance().unwrap();
            }
        }
        self.expect(TokenType::RPAREN, String::from(")"))?;
        let ret_type = if self.peek().unwrap().token_type == TokenType::ARROW{
            self.advance().unwrap();
            Some(self.advance().unwrap().value.clone())
        }else{
            None
        };

        let func = Function { fn_name: fn_name.clone(), paras: parameters.clone(), ret_type:ret_type.clone() };
        SYMBOL_TABLES.lock().unwrap().current_scope_mut().add_function(fn_name.clone(), func);
        SYMBOL_TABLES.lock().unwrap().push_scope();
        for i in &parameters{
            SYMBOL_TABLES.lock().unwrap().current_scope_mut().add_variable(i.1.clone(),i.0.clone());
        }
        SYMBOL_TABLES.lock().unwrap().pop_scope();
        let body = self.parse_block()?;
        
        Ok(ASTNode::FunctionDefinition { 
            fn_name, 
            parameters, 
            ret_type, 
            body ,
        })
    }

    fn parse_return(&mut self) -> Result<ASTNode, String>{
        self.expect(TokenType::KEYWORD, String::from("ret"))?;
        let value = self.parse_expression()?;
        
        self.expect(TokenType::SEMICOLON, String::from(";"))?;
        Ok(ASTNode::Return(Box::new(value.0)))

    }

    fn parse_function_call(&mut self, fn_name:String ) -> Result<(ASTNode, String), String>{
        self.expect(TokenType::LPAREN, String::from("("))?;
        let mut args = Vec::new();
      
        if !SYMBOL_TABLES.lock().unwrap().global_scope().has_function(fn_name.as_str()){
            return Err(format!("No function '{}' ", fn_name));
        }
        let info = get_fun(fn_name.clone());

        let mut tylist = Vec::new();

        for i in info.paras.clone(){
            tylist.push(i.0);
        }
        
        while self.peek().unwrap().token_type != TokenType::RPAREN{
            let arg = self.parse_expression()?.0;
            
            args.push(arg);

            if self.peek().unwrap().token_type == TokenType::COMMA{
                self.advance();
            }
        }

        if (args.len() != info.paras.len()){
            let len = info.paras.len();
            let alen = args.len();
            return Err(format!("function {fn_name} need {len} parameters, but find {alen}."));
        }

        self.expect(TokenType::RPAREN, String::from(")"))?;

        let ret_ty = match info.ret_type{
            Some(a) => a,
            None => "void".to_string(),
        };
       

        Ok((ASTNode::FunctionCall { fn_name: fn_name, argument: args }, ret_ty))
    }

    fn parse_variable_definition(&mut self) -> Result<ASTNode, String>{
        let var_type = handle_type(self.advance().unwrap().value.as_str())?;
        let identifier = handle_identifier(self.advance().unwrap().value.as_str())?;     
        let index = has_var(identifier.clone(), &mut current_index());

        let mut ty = String::new();
        let mut var_value = None;

        if index {
            return Err(format!("Variable '{}' is already defined", identifier));
        } 
        let var_info = if self.peek().unwrap().token_type == TokenType::EQUALS{
            self.advance();
            Some(Box::new(self.parse_expression()?))           
        } else{
            None
        };
        match var_info{
            Some(i ) => {
                let mut ty = i.1;
                if var_type != ty{
                    return Err(format!("cannot assign {} type to {} type", ty, var_type));
                } else{
                    var_value = Some(Box::new(i.0));
                }
            }
            None =>()
        }

        self.expect(TokenType::SEMICOLON, String::from(";"))?;
        insert_var(identifier.clone(), var_type.clone());
        Ok(ASTNode::VariableDefinition{ 
            var_type, 
            identifier, 
            var_value,
        })
    }

    fn parse_assignment(&mut self, var_name:String) ->Result<ASTNode, String>{
        let index = &mut current_index();
        if !has_var(var_name.clone(), index){
            return Err(format!("No variable {var_name}"));
        }
        self.expect(TokenType::EQUALS, String::from("="))?;
        let var_value = Some(Box::new(self.parse_expression()?.0));
        self.expect(TokenType::SEMICOLON, String::from(";"))?;
        Ok(ASTNode::Assignment {    
            identifier: var_name, 
            var_value,
        })
    }

    fn parse_expression_primary(&mut self) -> Result<(ASTNode, String), String>{
        let token = self.advance().unwrap().clone();
        match token.token_type {
            TokenType::NUMBER => Ok((ASTNode::Number(token.value.clone()), "i32".to_string())),
            TokenType::STRING => Ok((ASTNode::String(token.value.clone()), "str".to_string())),
            TokenType::ID => {
                
                if self.peek().unwrap().token_type == TokenType::LPAREN{
                    if !SYMBOL_TABLES.lock().unwrap().stack[0].has_function(token.value.as_str()) {
                        return Err(format!("No Function: '{}' ", token.value));
                    } 
                    self.parse_function_call(token.value)
                } else {
                    if !has_var(token.value.clone(), &mut current_index()){
                        return Err(format!("No such variable {}", &token.value));
                    }
                    let ty = get_ty(token.value.clone(), &mut current_index());
                    Ok((ASTNode::Identifier(token.value.clone()), ty))
                }
            }
            TokenType::AT => {
                
                if self.peek().unwrap().token_type == TokenType::ID{
                    let token = self.advance().unwrap().clone();
                    if self.peek().unwrap().token_type == TokenType::LPAREN{
                        if !SYMBOL_TABLES.lock().unwrap().stack[0].has_function(token.value.as_str()) {
                            return Err(format!("No Function: '{}' ", token.value));
                        } 
                        self.parse_function_call(token.value)
                    } else {
                        if !SYMBOL_TABLES.lock().unwrap().stack[0].has_variable(&token.value){
                            return Err(format!("No such variable {}", &token.value));
                        }
                        let ty = get_ty(token.value.clone(), &mut current_index());
                        Ok((ASTNode::Identifier(token.value.clone()), ty))
                    }

                }else {
                    return Err(format!("@ should before the the variable or function."))
                }
                 
               

            }
            TokenType::LPAREN => {
                let expr = self.parse_expression()?;
                self.expect(TokenType::RPAREN, String::from(")"))?;
                Ok(expr)
            },
            _ => Err(format!("Unexpected token: {:?}", token)),
        }
    }

    fn parse_expression_secondary(&mut self) -> Result<(ASTNode,String), String>{
        let mut primary = self.parse_expression_primary()?;

        while let Ok(token) = self.peek(){
            if token.token_type == TokenType::ASTERISK 
                || token.token_type == TokenType::SLASH 
                || token.token_type == TokenType::REM {
                let op = self.advance().unwrap().value.clone();
                let right_expr = self.parse_expression_primary()?;
                primary.0 = ASTNode::InfixExpression {
                    left_expr:Box::new(primary.0),
                    op,
                    right_expr:Box::new(right_expr.0),
                }
            } else{
                break;
            }
        }
        Ok(primary)
    }
    
    fn parse_expression_third(&mut self) -> Result<(ASTNode, String), String>{
        let mut primary = self.parse_expression_secondary()?;

        while let Ok(token) = self.peek(){
            if token.token_type == TokenType::DEQUALS 
                || token.token_type == TokenType::LT
                || token.token_type == TokenType::LE
                || token.token_type == TokenType::ST
                || token.token_type == TokenType::SE
                || token.token_type == TokenType::UNEQ
            {
                let op = self.advance().unwrap().value.clone();
                let right_expr = self.parse_expression_secondary()?;
                primary.0 = ASTNode::InfixExpression {
                    left_expr:Box::new(primary.0),
                    op,
                    right_expr:Box::new(right_expr.0),
                }
            } else{
                break;
            }
        }
        Ok(primary)
    }

    fn parse_expression(&mut self) -> Result<(ASTNode,String), String>{
        let mut node = self.parse_expression_third()?;

        while let Ok(token) = self.peek() {
            if token.token_type == TokenType::ADD || token.token_type == TokenType::MINUS{
                let op = self.advance().unwrap().value.clone();
                let right_expr = self.parse_expression_secondary()?;
                node.0 = ASTNode::InfixExpression {
                    left_expr:Box::new(node.0),
                    op,
                    right_expr:Box::new(right_expr.0),
                };
            } else{
                break;
            }
        }

        Ok((node.0, node.1))
    }

    fn parse_while(&mut self) -> Result<ASTNode, String>{
        self.expect(TokenType::KEYWORD, String::from("while"))?; 
           
        let condition = self.parse_expression()?;                
           
    
           
        let body = self.parse_block()?;                         
    
        Ok(ASTNode::While {
          condition: Box::new(condition.0),
          body,
        })

    }

    fn parse_if_else(&mut self) -> Result<ASTNode, String>{
        self.expect(TokenType::KEYWORD, String::from("if"))?; 
    
        let condition = self.parse_expression()?;             
    
        
        let if_body = self.parse_block()?;                    
    
        let mut else_body = None;
        let mut elif_body = Vec::new();
        let mut el_condition = Vec::new();
        loop{
            if self.peek().unwrap().token_type == TokenType::KEYWORD && self.peek().unwrap().value == "elif" {
                self.advance().unwrap(); 
                el_condition.push(self.parse_expression()?.0);
               
                if self.peek().unwrap().token_type == TokenType::LBRACE {
                    elif_body.push(self.parse_block()?); 
                } else {
                    return Err(format!("Expected block after 'elif'"));
                }
            
                if self.peek().unwrap().token_type == TokenType::KEYWORD 
                    && (self.peek().unwrap().value == "else"
                    || self.peek().unwrap().value == "elif"){
                        continue;
                }
                break;
            } else if self.peek().unwrap().token_type == TokenType::KEYWORD && self.peek().unwrap().value == "else"{
                self.advance().unwrap(); 
                if self.peek().unwrap().token_type == TokenType::LBRACE {
                    else_body = Some(self.parse_block()?); 
                } else {
                    return Err(format!("Expected block after 'else'"));
                }
                break;
            } else{
                break;
            }

        }
        
    
        Ok(ASTNode::IfElse {
            condition: Box::new(condition.0),
            if_body,
            elif_body,
            el_condition ,
            else_body,
        })

    }

    fn parse_block(&mut self) -> Result<Vec<ASTNode>, String>{
        SYMBOL_TABLES.lock().unwrap().push_scope();
       
       
        self.expect(TokenType::LBRACE, String::from("{"))?;
        let mut statements = Vec::new();
        while self.peek().unwrap().token_type != TokenType::RBRACE{
            statements.push(self.parse_statement()?);
        }
        self.expect(TokenType::RBRACE, String::from("}"))?;
        SYMBOL_TABLES.lock().unwrap().pop_scope();
        Ok(statements)
    }
}


fn handle_identifier(ident: &str) -> Result<String, String>{
    let keywords = ["i32", "str", "ret", "sub", "if","else", "while", "for", "in", "call"];
    if keywords.contains(&ident){
        Err(format!("{ident} is a keyword. Cannot use keyword as identifier."))
    }else{
        Ok(ident.to_string())
    }

}

fn handle_type(ty: &str) -> Result<String, String>{
    let keywords = ["i32", "str"];
    if keywords.contains(&ty){
        Ok(ty.to_string())
    }else{  
        Err(format!("{ty} is not a valid type."))
    }

}

#[cfg(test)]
mod tests{
    use super::*;

    #[test]
    fn peek(){
        let list = [
            Token{
                token_type: TokenType::KEYWORD,
                value:String::from("i32"),    
            },
            Token{
                token_type: TokenType::KEYWORD,
                value:String::from("i32"),    
            }
        ];
        let tokens = Vec::from(list);
        let parser = Parser::new(tokens.clone());
        let first = parser.peek().unwrap();
        let second = parser.peek().unwrap();
        assert_eq!(tokens[0], *first);
        assert_eq!(tokens[1], *second);
    }

    #[test]
    fn peek_overlist(){
        let list = [
            Token{
                token_type: TokenType::KEYWORD,
                value:String::from("i32"),    
            },
            Token{
                token_type: TokenType::KEYWORD,
                value:String::from("i32"),    
            }
        ];
        let tokens = Vec::from(list);
        let mut parser = Parser::new(tokens.clone());
        parser.current = 2;
        let third = parser.peek();

        assert_eq!(third, Err(Error::ParserErr));
        
    }

    #[test]
    fn advance(){
        let list = [
            Token{
                token_type: TokenType::KEYWORD,
                value:String::from("i32"),    
            },
            Token{
                token_type: TokenType::KEYWORD,
                value:String::from("i32"),    
            }
        ];
        let tokens = Vec::from(list);
        let mut parser = Parser::new(tokens.clone());
        
        let first = parser.advance().unwrap();
        assert_eq!(tokens[0], *first);
        let second = parser.advance().unwrap();
        assert_eq!(tokens[1], *second);
        let third = parser.advance();

        assert_eq!(third, Err(Error::ParserErr));
        
    }

    #[test]
    fn expect(){
        let list = [
            Token{
                token_type: TokenType::KEYWORD,
                value:String::from("i32"),    
            },
            Token{
                token_type: TokenType::KEYWORD,
                value:String::from("sub"),    
            }
        ];
        let tokens = Vec::from(list);
        let mut parser = Parser::new(tokens.clone());
        assert!(parser.expect(TokenType::KEYWORD, String::from("i32")).is_ok());
        assert!(parser.expect(TokenType::KEYWORD,String::from("sub")).is_ok());
    }

    #[test]
    fn functionDef(){
        let input ="sub main(i32 a, i32 b) -> i32{ ret a + b; }";
        println!("{input}");
    
        let mut lexer = LEXER::new(input);
        let mut tokens = Vec::new();

        tokens = tokenization(&mut lexer).unwrap();
   
        let mut parser = Parser::new(tokens);

        let result = parser.parse_function_definition();
    
        assert!(result.is_ok());
    
        if let ASTNode::FunctionDefinition { fn_name, parameters, ret_type, body } = result.unwrap() {

            assert_eq!(fn_name, "main");
    
            assert_eq!(parameters.len(), 2);
            assert_eq!(parameters[0], ("i32".to_string(), "a".to_string()));
            assert_eq!(parameters[1], ("i32".to_string(), "b".to_string()));
    
            assert_eq!(ret_type, Some("i32".to_string()));
    
            assert_eq!(body.len(), 1);
            if let ASTNode::Return(expr) = &body[0] {
                if let ASTNode::InfixExpression { left_expr, op, right_expr } = &**expr {
                    assert_eq!(**left_expr, ASTNode::Identifier("a".to_string()));
                    assert_eq!(op, "+");
                    assert_eq!(**right_expr, ASTNode::Identifier("b".to_string()));
                } else {
                    panic!("Expected InfixExpression inside Return.");
                }
            } else {
                panic!("Expected Return statement in function body.");
            }
        } else {
            panic!("Expected FunctionDefinition ASTNode.");
        }
      
        
    }


    #[test]
    fn cal_expr() {
  
        let input ="i32 a = 1 * 2 + (3 - 4) / 5;";
       
    
        let mut lexer = LEXER::new(input);
        let mut tokens = Vec::new();
    
        
    
    
        tokens = tokenization(&mut lexer).unwrap();
        tokens.push(Token {
            token_type: TokenType::EOF,
            value: String::new(), 
        });
        
    
        let mut parser = Parser::new(tokens.clone());

        if let ASTNode::Program(statements) = parser.parse_program().unwrap() {
            assert_eq!(statements.len(), 1); 

            if let ASTNode::VariableDefinition { var_type, identifier, var_value } = &statements[0] {
                assert_eq!(var_type, "i32");
                assert_eq!(identifier, "a");

                if let Some(value) = var_value {
                    if let ASTNode::InfixExpression { left_expr, op, right_expr } = &**value {
                      
                        assert!(matches!(**left_expr, ASTNode::InfixExpression { .. }));
                        if let ASTNode::InfixExpression { left_expr, op, right_expr } = &**left_expr {
                     
                            assert!(matches!(**left_expr, ASTNode::Number(ref n) if n == "1"));
                            assert_eq!(op, "*");
                            assert!(matches!(**right_expr, ASTNode::Number(ref n) if n == "2"));
                        }

                        assert_eq!(op, "+");
                        if let ASTNode::InfixExpression { left_expr, op, right_expr } = &**right_expr {
                            assert!(matches!(**left_expr, ASTNode::InfixExpression { .. }));
                            if let ASTNode::InfixExpression { left_expr, op, right_expr } = &**left_expr {    
                                assert!(matches!(**left_expr, ASTNode::Number(ref n) if n == "3"));
                                assert_eq!(op, "-");
                                assert!(matches!(**right_expr, ASTNode::Number(ref n) if n == "4"));
                            }
                            assert_eq!(op, "/");
                            assert!(matches!(**right_expr, ASTNode::Number(ref n) if n == "5"));
                        }
                    }
                } else {
                    panic!("Expected a value in assignment, but found None");
                }
            } else {
                panic!("Expected an assignment statement");
            }
        } else {
            panic!("Expected program node");
        }
    }

  
}
